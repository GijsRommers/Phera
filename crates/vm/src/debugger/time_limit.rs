//! A stopped request keeps its wall-clock budget while the IDE is deciding what to do.

use std::cell::Cell;
use std::future::Future;
use std::rc::Rc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

/// Request time with debugger stops subtracted. Used only by the debug-enabled server.
#[derive(Clone)]
pub struct DebugTimeLimit(Rc<Clock>);

struct Clock {
    remaining: Cell<Duration>,
    running_since: Cell<Option<Instant>>,
    changed: Notify,
}

impl DebugTimeLimit {
    pub fn new(limit: Duration) -> Self {
        Self(Rc::new(Clock {
            remaining: Cell::new(limit),
            running_since: Cell::new(Some(Instant::now())),
            changed: Notify::new(),
        }))
    }

    pub async fn run<T>(&self, future: impl Future<Output = T>) -> Option<T> {
        tokio::select! {
            result = future => Some(result),
            () = self.expire() => None,
        }
    }

    async fn expire(&self) {
        loop {
            let changed = self.0.changed.notified();
            let Some(started) = self.0.running_since.get() else {
                changed.await;
                continue;
            };
            let deadline = started + self.0.remaining.get();
            tokio::select! {
                biased;
                () = changed => {}
                () = tokio::time::sleep_until(deadline) => return,
            }
        }
    }

    pub(super) fn pause(&self) -> Paused {
        if let Some(started) = self.0.running_since.take() {
            self.0
                .remaining
                .set(self.0.remaining.get().saturating_sub(started.elapsed()));
            self.0.changed.notify_one();
        }
        Paused(self.clone())
    }
}

pub(super) struct Paused(DebugTimeLimit);

impl Drop for Paused {
    fn drop(&mut self) {
        self.0.0.running_since.set(Some(Instant::now()));
        self.0.0.changed.notify_one();
    }
}
