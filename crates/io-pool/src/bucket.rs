//! Shared accounting: permits cover connecting, borrowed, resetting and closing sockets.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, timeout};

use crate::{Connection, Options};
use tokio::runtime::Handle;

pub struct Idle<C> {
    pub driver: C,
    pub born: Instant,
    pub returned: Instant,
}

pub struct Bucket<C: Connection> {
    pub options: Options,
    pub capacity: Arc<Semaphore>,
    runtime: Handle,
    idle: Mutex<VecDeque<Idle<C>>>,
}

impl<C: Connection> Bucket<C> {
    pub fn new(options: Options) -> Arc<Self> {
        let bucket = Arc::new(Self {
            options,
            runtime: Handle::current(),
            capacity: Arc::new(Semaphore::new(options.size)),
            idle: Mutex::new(VecDeque::new()),
        });
        let weak = Arc::downgrade(&bucket);
        // This task owns only a weak reference between ticks, so isolated test
        // pools can disappear. Production pools are retained by the registry.
        tokio::spawn(async move {
            let interval = options
                .idle
                .min(options.lifetime)
                .min(Duration::from_secs(1));
            loop {
                tokio::time::sleep(interval).await;
                let Some(bucket) = weak.upgrade() else { return };
                bucket.expire().await;
            }
        });
        bucket
    }

    pub async fn take_idle(&self) -> Option<Idle<C>> {
        self.idle.lock().await.pop_front()
    }

    pub fn expired(&self, idle: &Idle<C>) -> bool {
        let now = Instant::now();
        now.duration_since(idle.born) >= self.options.lifetime
            || now.duration_since(idle.returned) >= self.options.idle
    }

    async fn expire(&self) {
        loop {
            // Closing occupies capacity too: a new handshake must not race a
            // closing socket and exceed the server-visible maximum.
            let Ok(permit) = self.capacity.clone().try_acquire_owned() else {
                return;
            };
            let expired = {
                let mut idle = self.idle.lock().await;
                idle.iter()
                    .position(|entry| self.expired(entry))
                    .and_then(|index| idle.remove(index))
            };
            let Some(expired) = expired else { return };
            expired.driver.close().await;
            drop(permit);
        }
    }

    pub async fn recycle(&self, mut driver: C, born: Instant, permit: OwnedSemaphorePermit) {
        let reusable = driver.reusable()
            && born.elapsed() < self.options.lifetime
            && matches!(
                timeout(Duration::from_secs(5), driver.reset()).await,
                Ok(Ok(()))
            );
        if reusable && born.elapsed() < self.options.lifetime {
            self.idle.lock().await.push_back(Idle {
                driver,
                born,
                returned: Instant::now(),
            });
        } else {
            driver.close().await;
        }
        drop(permit);
    }
}

impl<C: Connection> Drop for Bucket<C> {
    fn drop(&mut self) {
        let idle = std::mem::take(self.idle.get_mut());
        {
            self.runtime.spawn(async move {
                for connection in idle {
                    connection.driver.close().await;
                }
            });
        }
    }
}
