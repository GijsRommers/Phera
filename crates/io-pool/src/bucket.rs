//! The connections of one pool key: the permits that bound how many are open,
//! and the idle ones waiting to be lent again.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Weak};
use std::time::Duration;

use tokio::runtime::Handle;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, timeout};

use crate::lease::{Lease, Owner};
use crate::{Connection, Options};

/// An idle check or a reset that takes longer than this counts as failed, so
/// a dead server cannot hold a permit for long.
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// The longest pause between two looks for idle connections to expire.
const EXPIRY_INTERVAL: Duration = Duration::from_secs(1);

struct Idle<C> {
    connection: C,
    /// When the connection was opened, for `Options::lifetime`.
    born: Instant,
    /// When it was last returned, for `Options::idle`.
    returned: Instant,
}

pub(crate) struct Bucket<C: Connection> {
    pub(crate) options: Options,
    /// One permit per connection that is being opened, is lent, or is being
    /// reset or closed. Waiting for a permit is first come, first served.
    pub(crate) capacity: Arc<Semaphore>,
    /// The connection returned first is lent first.
    idle: Mutex<VecDeque<Idle<C>>>,
    /// The I/O runtime, where idle connections are closed when the bucket goes.
    runtime: Handle,
}

impl<C: Connection> Bucket<C> {
    pub(crate) fn new(options: Options, runtime: Handle) -> Arc<Self> {
        let bucket = Arc::new(Self {
            options,
            capacity: Arc::new(Semaphore::new(options.size)),
            idle: Mutex::new(VecDeque::new()),
            runtime: runtime.clone(),
        });
        let interval = options.idle.min(options.lifetime).min(EXPIRY_INTERVAL);
        runtime.spawn(expire_idle(Arc::downgrade(&bucket), interval));
        bucket
    }

    /// Lends an idle connection that passes its check, or opens a new one
    /// with `connect`. The caller already holds `permit`, which the lease
    /// keeps until the connection is back.
    pub(crate) async fn lend(
        self: Arc<Self>,
        permit: OwnedSemaphorePermit,
        connect: impl Future<Output = Result<C, C::Error>>,
    ) -> Result<Lease<C>, C::Error> {
        while let Some(mut idle) = self.take_idle().await {
            let usable = !self.expired(&idle)
                && matches!(
                    timeout(
                        CHECK_TIMEOUT,
                        idle.connection.check(idle.returned.elapsed())
                    )
                    .await,
                    Ok(true)
                );
            if usable {
                let owner = Owner::new(self, permit, idle.born);
                return Ok(Lease::new(idle.connection, Some(owner)));
            }
            idle.connection.close().await;
        }
        let connection = connect.await?;
        Ok(Lease::new(
            connection,
            Some(Owner::new(self, permit, Instant::now())),
        ))
    }

    /// Takes back a connection whose last handle is gone: reset and kept for
    /// the next borrower, or closed when it is too old or cannot be reset.
    /// The permit is released only after that, so a closing connection still
    /// counts against the limit.
    pub(crate) async fn recycle(
        &self,
        mut connection: C,
        born: Instant,
        permit: OwnedSemaphorePermit,
    ) {
        let young = || born.elapsed() < self.options.lifetime;
        // Too old means no reset is tried; a reset that crosses the lifetime
        // still retires the connection.
        let keep = connection.reusable()
            && young()
            && matches!(timeout(CHECK_TIMEOUT, connection.reset()).await, Ok(Ok(())))
            && young();
        if keep {
            self.idle.lock().await.push_back(Idle {
                connection,
                born,
                returned: Instant::now(),
            });
        } else {
            connection.close().await;
        }
        drop(permit);
    }

    async fn take_idle(&self) -> Option<Idle<C>> {
        self.idle.lock().await.pop_front()
    }

    fn expired(&self, idle: &Idle<C>) -> bool {
        idle.born.elapsed() >= self.options.lifetime || idle.returned.elapsed() >= self.options.idle
    }

    /// Closes idle connections that are too old or idle for too long. Each
    /// close takes a free permit first, so that a new handshake cannot
    /// overlap a closing socket and exceed the limit the server sees; with no
    /// permit free, this waits for the next round.
    async fn close_expired(&self) {
        while let Ok(permit) = self.capacity.clone().try_acquire_owned() {
            let expired = {
                let mut idle = self.idle.lock().await;
                idle.iter()
                    .position(|idle| self.expired(idle))
                    .and_then(|index| idle.remove(index))
            };
            let Some(expired) = expired else { return };
            expired.connection.close().await;
            drop(permit);
        }
    }
}

/// Closes the idle connections of a bucket that is no longer used, such as an
/// isolated pool in a test. Pools in statics live for the whole process.
impl<C: Connection> Drop for Bucket<C> {
    fn drop(&mut self) {
        let idle = std::mem::take(self.idle.get_mut());
        self.runtime.spawn(async move {
            for idle in idle {
                idle.connection.close().await;
            }
        });
    }
}

/// Runs for as long as the bucket exists. Between rounds it holds only a weak
/// reference, so it does not keep an unused bucket alive.
async fn expire_idle<C: Connection>(bucket: Weak<Bucket<C>>, interval: Duration) {
    loop {
        tokio::time::sleep(interval).await;
        let Some(bucket) = bucket.upgrade() else {
            return;
        };
        bucket.close_expired().await;
    }
}
