//! A worker can lend an idle connection to its next `new PDO($dsn)`.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread::{self, ThreadId};
use std::time::Duration;

use phera_io_pool::{Connection, Options};
use tokio::runtime::{Handle, Id};
use tokio::sync::Notify;
use tokio::time::{Instant, timeout};

use super::admission::{Admission, Grant};
use super::lease::Lease;
use super::socket::{Socket, closed};
use super::{Driver, Key};
use crate::errors::DbError;

const CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const EXPIRY_INTERVAL: Duration = Duration::from_secs(1);

thread_local! {
    static BUCKETS: RefCell<Vec<Weak<Worker>>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct WorkerId {
    pub(super) thread: ThreadId,
    pub(super) runtime: Id,
}

pub(super) struct Worker {
    id: WorkerId,
    admission: Arc<Admission>,
    options: Options,
    idle: Mutex<Slots>,
    changed: Arc<Notify>,
}

pub(super) struct Idle {
    pub(super) key: Arc<Key>,
    socket: Socket,
    returned: Instant,
}

#[derive(Default)]
struct Slots {
    connections: Vec<Option<Idle>>,
    free: Vec<usize>,
}

impl Slots {
    fn take(&mut self, matches: impl Fn(&Idle) -> bool) -> Option<Idle> {
        let (index, _) = self
            .connections
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| Some((index, slot.as_ref()?)))
            .filter(|(_, idle)| matches(idle))
            .min_by_key(|(_, idle)| idle.returned)?;
        self.free.push(index);
        self.connections[index].take()
    }

    fn insert(&mut self, idle: Idle) {
        if let Some(index) = self.free.pop() {
            self.connections[index] = Some(idle);
        } else {
            self.connections.push(Some(idle));
        }
    }
}

impl Worker {
    pub(super) fn current(admission: Arc<Admission>, options: Options) -> Arc<Self> {
        let runtime = Handle::current();
        let id = WorkerId {
            thread: thread::current().id(),
            runtime: runtime.id(),
        };
        BUCKETS.with_borrow_mut(|workers| {
            workers.retain(|worker| worker.strong_count() != 0);
            if let Some(worker) = workers
                .iter()
                .filter_map(Weak::upgrade)
                .find(|worker| worker.id == id && Arc::ptr_eq(&worker.admission, &admission))
            {
                return worker;
            }
            let worker = Arc::new(Self {
                id,
                admission,
                options,
                idle: Mutex::new(Slots::default()),
                changed: Arc::new(Notify::new()),
            });
            workers.push(Arc::downgrade(&worker));
            worker.admission.register(&worker.changed);
            runtime.spawn(worker.clone().maintain());
            worker
        })
    }

    fn idle(&self) -> MutexGuard<'_, Slots> {
        self.idle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(super) async fn lend(
        self: Arc<Self>,
        key: Arc<Key>,
        connect: impl Future<Output = Result<Driver, DbError>>,
    ) -> Result<Lease, DbError> {
        loop {
            let local = self.idle().take(|idle| idle.key == key);
            let grant = match local {
                Some(idle) => Grant::Idle(idle),
                None => self
                    .admission
                    .acquire(self.id, key.clone())
                    .await
                    .ok_or_else(closed)?,
            };
            let socket = match grant {
                Grant::Open(permit) => Socket::new(connect.await?, Some(permit)),
                Grant::Idle(mut idle) => {
                    let usable = !self.expired(&idle)
                        && matches!(
                            timeout(
                                CHECK_TIMEOUT,
                                idle.socket.driver_mut()?.check(idle.returned.elapsed())
                            )
                            .await,
                            Ok(true)
                        );
                    if !usable {
                        idle.socket.close().await;
                        continue;
                    }
                    idle.socket
                }
            };
            return Ok(Lease::new(socket, Some((self, key))));
        }
    }

    pub(super) async fn recycle(&self, mut socket: Socket, key: Arc<Key>) {
        let keep = socket.born.elapsed() < self.options.lifetime
            && !self.admission.is_closed()
            && match socket.driver_mut() {
                Ok(driver) if Connection::reusable(driver) => {
                    matches!(timeout(CHECK_TIMEOUT, driver.reset()).await, Ok(Ok(())))
                }
                _ => false,
            }
            && socket.born.elapsed() < self.options.lifetime;
        if keep {
            let idle = Idle {
                key,
                socket,
                returned: Instant::now(),
            };
            if let Some(idle) = self.admission.offer(self.id, idle) {
                if let Some(reclaim) = self.admission.reclaim() {
                    idle.socket.close().await;
                    drop(reclaim);
                } else {
                    self.idle().insert(idle);
                    self.changed.notify_one();
                }
            }
        } else {
            socket.close().await;
        }
    }

    fn expired(&self, idle: &Idle) -> bool {
        idle.socket.born.elapsed() >= self.options.lifetime
            || idle.returned.elapsed() >= self.options.idle
    }

    async fn maintain(self: Arc<Self>) {
        let interval = self
            .options
            .idle
            .min(self.options.lifetime)
            .min(EXPIRY_INTERVAL);
        loop {
            let closing = self.admission.is_closed();
            while let Some(expired) = self.take_expired(closing) {
                expired.socket.close().await;
            }
            if closing {
                return;
            }
            let reclaim = {
                let mut idle = self.idle();
                if idle.connections.len() == idle.free.len() {
                    None
                } else if let Some(reclaim) = self.admission.reclaim() {
                    idle.take(|_| true).map(|idle| (idle, reclaim))
                } else {
                    None
                }
            };
            if let Some((idle, reclaim)) = reclaim {
                idle.socket.close().await;
                drop(reclaim);
                continue;
            }
            tokio::select! {
                _ = self.changed.notified() => {}
                _ = tokio::time::sleep(interval) => {}
            }
        }
    }

    fn take_expired(&self, closing: bool) -> Option<Idle> {
        self.idle().take(|idle| closing || self.expired(idle))
    }
}
