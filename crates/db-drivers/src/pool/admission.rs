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
    waiter_count: AtomicUsize,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    waiters: VecDeque<Waiter>,
    workers: Vec<Weak<Notify>>,
    /// Number of currently open sockets (leased or idle) per worker.
    open_by_worker: Vec<(WorkerId, usize)>,
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
pub(super) struct Permit {
    admission: Option<Arc<Admission>>,
    worker: WorkerId,
}

/// Only one worker needs to give up an idle socket for the first waiter.
pub(super) struct Reclaim(Arc<Admission>);

impl Admission {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            open: AtomicUsize::new(0),
            waiter_count: AtomicUsize::new(0),
            state: Mutex::new(State::default()),
        }
    }

    #[inline]
    pub(super) fn has_waiters_fast(&self) -> bool {
        self.waiter_count.load(Ordering::SeqCst) > 0
    }

    #[inline]
    fn sync_waiters(&self, state: &State) {
        self.waiter_count
            .store(state.waiters.len(), Ordering::SeqCst);
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
                self.sync_waiters(&state);
                self.open.fetch_add(1, Ordering::Relaxed);
                state.inc_open(worker);
                return Some(Grant::Open(Permit {
                    admission: Some(self.clone()),
                    worker,
                }));
            }
            let (grant, receiver) = oneshot::channel();
            state.waiters.push_back(Waiter { worker, key, grant });
            self.sync_waiters(&state);
            state.wake_workers();
            receiver
        };
        receiver.await.ok()
    }

    /// Give a reset connection back directly to a waiting request on this worker
    /// when doing so does not starve another worker or bypass a different key.
    pub(super) fn offer(&self, worker: WorkerId, mut idle: Idle) -> Option<Idle> {
        let mut state = self.state();
        while let Some(index) = state.select_offer_waiter(worker, &idle.key, self.limit) {
            let Some(waiter) = state.waiters.remove(index) else {
                break;
            };
            match waiter.grant.send(Grant::Idle(idle)) {
                Ok(()) => {
                    self.sync_waiters(&state);
                    return None;
                }
                Err(Grant::Idle(returned)) => idle = returned,
                Err(Grant::Open(_)) => {
                    self.sync_waiters(&state);
                    return None;
                }
            }
        }
        self.sync_waiters(&state);
        if !state.waiters.is_empty() {
            state.wake_workers();
        }
        Some(idle)
    }

    pub(super) fn reclaim(self: &Arc<Self>) -> Option<Reclaim> {
        let mut state = self.state();
        state.remove_cancelled();
        self.sync_waiters(&state);
        if state.reclaiming || state.waiters.is_empty() {
            return None;
        }
        state.reclaiming = true;
        Some(Reclaim(self.clone()))
    }

    /// Reclaim a freshly-returned socket immediately only when another key is
    /// at the head, the pool limit is smaller than the active worker count, or
    /// this worker holds more than its fair share. Otherwise keep it warm on
    /// this worker briefly so rapid sequential requests do not ping-pong live
    /// connections between workers.
    pub(super) fn reclaim_on_return(
        self: &Arc<Self>,
        worker: WorkerId,
        key: &Key,
    ) -> Option<Reclaim> {
        let mut state = self.state();
        let should = state.should_reclaim_immediately(worker, key, self.limit);
        self.sync_waiters(&state);
        if !should || state.reclaiming {
            return None;
        }
        state.reclaiming = true;
        Some(Reclaim(self.clone()))
    }

    pub(super) fn has_waiters(&self) -> bool {
        let mut state = self.state();
        state.remove_cancelled();
        self.sync_waiters(&state);
        !state.waiters.is_empty()
    }

    pub(super) fn is_closed(&self) -> bool {
        self.state().closed
    }

    pub(super) fn shutdown(&self) {
        let mut state = self.state();
        state.closed = true;
        state.waiters.clear();
        self.sync_waiters(&state);
        state.wake_workers();
    }

    fn release(self: &Arc<Self>, owner: WorkerId) {
        let mut state = self.state();
        state.dec_open(owner);
        while let Some(waiter) = state.waiters.pop_front() {
            let next_worker = waiter.worker;
            // Transfer the reservation without changing the open count.
            match waiter.grant.send(Grant::Open(Permit {
                admission: Some(self.clone()),
                worker: next_worker,
            })) {
                Ok(()) => {
                    state.inc_open(next_worker);
                    self.sync_waiters(&state);
                    state.wake_workers();
                    return;
                }
                Err(Grant::Open(mut permit)) => {
                    // The receiver is gone. This function still owns the reservation.
                    permit.admission.take();
                }
                Err(Grant::Idle(_)) => {}
            }
        }
        self.sync_waiters(&state);
        self.open.fetch_sub(1, Ordering::Relaxed);
    }
}

impl State {
    fn open_of(&self, worker: WorkerId) -> usize {
        self.open_by_worker
            .iter()
            .find_map(|&(id, count)| (id == worker).then_some(count))
            .unwrap_or(0)
    }

    fn inc_open(&mut self, worker: WorkerId) {
        if let Some((_, count)) = self.open_by_worker.iter_mut().find(|(id, _)| *id == worker) {
            *count += 1;
        } else {
            self.open_by_worker.push((worker, 1));
        }
    }

    fn dec_open(&mut self, worker: WorkerId) {
        if let Some(pos) = self.open_by_worker.iter().position(|(id, _)| *id == worker) {
            if self.open_by_worker[pos].1 <= 1 {
                self.open_by_worker.swap_remove(pos);
            } else {
                self.open_by_worker[pos].1 -= 1;
            }
        }
    }

    /// Distinct workers that currently hold open sockets or have live waiters.
    fn active_worker_count(&self, current: WorkerId) -> usize {
        let mut seen = Vec::with_capacity(self.open_by_worker.len() + 2);
        seen.push(current);
        for &(worker, count) in &self.open_by_worker {
            if count > 0 && !seen.contains(&worker) {
                seen.push(worker);
            }
        }
        for waiter in &self.waiters {
            if !waiter.grant.is_closed() && !seen.contains(&waiter.worker) {
                seen.push(waiter.worker);
            }
        }
        seen.len()
    }

    /// Picks a live waiter on `worker` that can reuse an idle socket for `key`,
    /// or returns `None` when the socket should instead be reclaimed for the
    /// head waiter (different key, or an under-served worker).
    fn select_offer_waiter(&mut self, worker: WorkerId, key: &Key, limit: usize) -> Option<usize> {
        self.remove_cancelled();
        let front = self.waiters.front()?;
        if *front.key != *key {
            return None;
        }
        if front.worker == worker {
            return Some(0);
        }
        // Another worker is waiting at the head for the same key. Reclaim this
        // socket only when the pool cannot give every active worker a socket,
        // or when this worker holds more than its fair share while the head
        // worker holds less. In balanced steady state, hand the socket directly
        // to this worker's next waiter so neither worker tears down live sockets.
        let active = self.active_worker_count(worker);
        let must_yield = if limit < active {
            true
        } else {
            let fair_share = limit / active.max(1);
            self.open_of(worker) > fair_share && self.open_of(front.worker) < fair_share
        };
        if must_yield {
            return None;
        }
        self.waiters.iter().position(|waiter| {
            !waiter.grant.is_closed() && waiter.worker == worker && *waiter.key == *key
        })
    }

    fn should_reclaim_immediately(&mut self, worker: WorkerId, key: &Key, limit: usize) -> bool {
        self.remove_cancelled();
        let Some(front) = self.waiters.front() else {
            return false;
        };
        if *front.key != *key {
            return true;
        }
        let active = self.active_worker_count(worker);
        if limit < active {
            return true;
        }
        let fair_share = limit / active.max(1);
        self.open_of(worker) > fair_share
    }

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
        if let Some(admission) = self.admission.take() {
            admission.release(self.worker);
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::connect_options::ConnectOptions;
    use crate::dsn::{DriverName, Dsn};
    use crate::pool::Policy;

    fn test_key(db: &str) -> Arc<Key> {
        let mut fields = BTreeMap::new();
        fields.insert("host".into(), "127.0.0.1".into());
        fields.insert("port".into(), "5432".into());
        fields.insert("dbname".into(), db.into());
        let dsn = Dsn {
            driver: DriverName::Postgres,
            fields,
            options: ConnectOptions::default(),
        };
        Arc::new(Key::new(&dsn, "user", "pass", Policy::Pooled))
    }

    fn two_worker_ids() -> (WorkerId, WorkerId) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let runtime = rt.handle().id();
        let w0 = WorkerId {
            thread: std::thread::current().id(),
            runtime,
        };
        let other = std::thread::spawn(|| std::thread::current().id())
            .join()
            .unwrap();
        let w1 = WorkerId {
            thread: other,
            runtime,
        };
        (w0, w1)
    }

    #[test]
    fn balanced_workers_reuse_local_waiter_without_thrash() {
        let (w0, w1) = two_worker_ids();
        let key = test_key("app");
        let mut state = State::default();
        state.inc_open(w0);
        state.inc_open(w0);
        state.inc_open(w1);
        state.inc_open(w1);

        let (tx1, _rx1) = oneshot::channel();
        let (tx0, _rx0) = oneshot::channel();
        state.waiters.push_back(Waiter {
            worker: w1,
            key: key.clone(),
            grant: tx1,
        });
        state.waiters.push_back(Waiter {
            worker: w0,
            key: key.clone(),
            grant: tx0,
        });

        // Balanced (2 each, fair_share = 2): w0 skips w1 at front and hands directly to w0.
        assert_eq!(state.select_offer_waiter(w0, &key, 4), Some(1));
    }

    #[test]
    fn oversubscribed_worker_yields_to_starved_worker_until_balanced() {
        let (w0, w1) = two_worker_ids();
        let key = test_key("app");
        let mut state = State::default();
        for _ in 0..4 {
            state.inc_open(w0);
        }

        let (tx1, _rx1) = oneshot::channel();
        let (tx0, _rx0) = oneshot::channel();
        state.waiters.push_back(Waiter {
            worker: w1,
            key: key.clone(),
            grant: tx1,
        });
        state.waiters.push_back(Waiter {
            worker: w0,
            key: key.clone(),
            grant: tx0,
        });

        // w0 holds 4 > fair_share (2) and w1 holds 0 < 2: w0 yields so w1 gets a slot.
        assert_eq!(state.select_offer_waiter(w0, &key, 4), None);

        // After yielding one socket (w0=3, w1=1), w0 still yields until 2/2.
        state.dec_open(w0);
        state.inc_open(w1);
        assert_eq!(state.select_offer_waiter(w0, &key, 4), None);

        // Once at 2/2, w0 stops yielding and reuses directly.
        state.dec_open(w0);
        state.inc_open(w1);
        assert_eq!(state.select_offer_waiter(w0, &key, 4), Some(1));
    }

    #[test]
    fn limit_below_worker_count_preserves_fifo() {
        let (w0, w1) = two_worker_ids();
        let key = test_key("app");
        let mut state = State::default();
        state.inc_open(w0);

        let (tx1, _rx1) = oneshot::channel();
        let (tx0, _rx0) = oneshot::channel();
        state.waiters.push_back(Waiter {
            worker: w1,
            key: key.clone(),
            grant: tx1,
        });
        state.waiters.push_back(Waiter {
            worker: w0,
            key: key.clone(),
            grant: tx0,
        });

        // Only 1 socket for 2 active workers: must yield so w1 is never starved.
        assert_eq!(state.select_offer_waiter(w0, &key, 1), None);
    }

    #[test]
    fn different_key_at_front_preserves_fifo() {
        let (w0, _w1) = two_worker_ids();
        let key_a = test_key("app_a");
        let key_b = test_key("app_b");
        let mut state = State::default();
        state.inc_open(w0);

        let (tx_b, _rx_b) = oneshot::channel();
        let (tx_a, _rx_a) = oneshot::channel();
        state.waiters.push_back(Waiter {
            worker: w0,
            key: key_b,
            grant: tx_b,
        });
        state.waiters.push_back(Waiter {
            worker: w0,
            key: key_a.clone(),
            grant: tx_a,
        });

        assert_eq!(state.select_offer_waiter(w0, &key_a, 4), None);
    }

    #[test]
    fn balanced_worker_holds_returned_socket_instead_of_ping_pong_reclaim() {
        let (w0, w1) = two_worker_ids();
        let key = test_key("app");
        let mut state = State::default();
        state.inc_open(w0);
        state.inc_open(w0);
        state.inc_open(w1);
        state.inc_open(w1);

        // w1 has a waiter while w0 momentarily has none, both at fair_share (2).
        let (tx1, _rx1) = oneshot::channel();
        state.waiters.push_back(Waiter {
            worker: w1,
            key: key.clone(),
            grant: tx1,
        });
        assert!(!state.should_reclaim_immediately(w0, &key, 4));

        // If w0 holds more than fair_share (3 > 2), it yields immediately.
        state.inc_open(w0);
        assert!(state.should_reclaim_immediately(w0, &key, 4));
    }
}
