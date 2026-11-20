//! Process-wide admission for the sockets behind `new PDO($dsn)`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use tokio::sync::{Notify, oneshot};

use super::Key;
use super::worker::{Idle, WorkerId};

pub(super) struct Admission {
    limit: usize,
    open: AtomicUsize,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    waiters: VecDeque<Waiter>,
    workers: Vec<Weak<Notify>>,
    reclaiming: bool,
    closed: bool,
}

struct Waiter {
    worker: WorkerId,
    key: Arc<Key>,
    grant: oneshot::Sender<Grant>,
}

pub(super) enum Grant {
    Open(Permit),
    Idle(Idle),
}

/// A reservation lasts from before the handshake until the socket closes.
pub(super) struct Permit(Option<Arc<Admission>>);

/// Only one worker needs to give up an idle socket for the first waiter.
pub(super) struct Reclaim(Arc<Admission>);

impl Admission {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            open: AtomicUsize::new(0),
            state: Mutex::new(State::default()),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) fn register(&self, worker: &Arc<Notify>) {
        let mut state = self.state();
        state.workers.retain(|worker| worker.strong_count() != 0);
        state.workers.push(Arc::downgrade(worker));
    }

    pub(super) async fn acquire(
        self: &Arc<Self>,
        worker: WorkerId,
        key: Arc<Key>,
    ) -> Option<Grant> {
        let receiver = {
            let mut state = self.state();
            if state.closed {
                return None;
            }
            state.remove_cancelled();
            if state.waiters.is_empty() && self.open.load(Ordering::Relaxed) < self.limit {
                self.open.fetch_add(1, Ordering::Relaxed);
                return Some(Grant::Open(Permit(Some(self.clone()))));
            }
            let (grant, receiver) = oneshot::channel();
            state.waiters.push_back(Waiter { worker, key, grant });
            state.wake_workers();
            receiver
        };
        receiver.await.ok()
    }

    /// Give a reset connection back directly when the first waiter can use it.
    pub(super) fn offer(&self, worker: WorkerId, mut idle: Idle) -> Option<Idle> {
        let mut state = self.state();
        while let Some(waiter) = state.waiters.front() {
            if waiter.grant.is_closed() {
                state.waiters.pop_front();
                continue;
            }
            if waiter.worker != worker || waiter.key != idle.key {
                break;
            }
            let Some(waiter) = state.waiters.pop_front() else {
                break;
            };
            match waiter.grant.send(Grant::Idle(idle)) {
                Ok(()) => return None,
                Err(Grant::Idle(returned)) => idle = returned,
                Err(Grant::Open(_)) => return None,
            }
        }
        if !state.waiters.is_empty() {
            state.wake_workers();
        }
        Some(idle)
    }

    pub(super) fn reclaim(self: &Arc<Self>) -> Option<Reclaim> {
        let mut state = self.state();
        state.remove_cancelled();
        if state.reclaiming || state.waiters.is_empty() {
            return None;
        }
        state.reclaiming = true;
        Some(Reclaim(self.clone()))
    }

    pub(super) fn is_closed(&self) -> bool {
        self.state().closed
    }

    pub(super) fn shutdown(&self) {
        let mut state = self.state();
        state.closed = true;
        state.waiters.clear();
        state.wake_workers();
    }

    fn release(self: &Arc<Self>) {
        let mut state = self.state();
        while let Some(waiter) = state.waiters.pop_front() {
            // Transfer the reservation without changing the open count.
            match waiter.grant.send(Grant::Open(Permit(Some(self.clone())))) {
                Ok(()) => {
                    state.wake_workers();
                    return;
                }
                Err(Grant::Open(mut permit)) => {
                    // The receiver is gone. This function still owns the reservation.
                    permit.0.take();
                }
                Err(Grant::Idle(_)) => {}
            }
        }
        self.open.fetch_sub(1, Ordering::Relaxed);
    }
}

impl State {
    fn remove_cancelled(&mut self) {
        while self
            .waiters
            .front()
            .is_some_and(|first| first.grant.is_closed())
        {
            self.waiters.pop_front();
        }
    }

    fn wake_workers(&self) {
        for worker in self.workers.iter().filter_map(Weak::upgrade) {
            worker.notify_one();
        }
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(admission) = self.0.take() {
            admission.release();
        }
    }
}

impl Drop for Reclaim {
    fn drop(&mut self) {
        let mut state = self.0.state();
        state.reclaiming = false;
        if !state.waiters.is_empty() {
            state.wake_workers();
        }
    }
}
