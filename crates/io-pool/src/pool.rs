//! A process-wide pool: one bucket of connections per key, all owned by one
//! I/O runtime. An extension keeps its pool in a `static`.

use std::collections::HashMap;
use std::future::Future;
use std::hash::Hash;
use std::sync::{Arc, OnceLock};

use tokio::runtime::Handle;
use tokio::sync::{Mutex, oneshot};
use tokio::time::timeout;

use crate::bucket::Bucket;
use crate::lease::Lease;
use crate::{Connection, IoRuntime, Options};

pub struct Pool<K, C: Connection> {
    runtime: &'static IoRuntime,
    /// Where [`Options::from_environment`] looks, and what it falls back to.
    prefix: &'static str,
    defaults: Options,
    options: OnceLock<Result<Options, String>>,
    buckets: OnceLock<Mutex<HashMap<K, Arc<Bucket<C>>>>>,
}

/// Why [`Pool::acquire`] gave no connection.
#[derive(Debug)]
pub enum AcquireError<E> {
    /// An environment setting is not a valid number.
    Settings(String),
    /// The I/O runtime could not start, or lost the task.
    Runtime(String),
    /// Every connection stayed busy for the whole `Options::wait`.
    Exhausted { limit: usize },
    /// Opening a new connection failed.
    Connect(E),
}

impl<K, C> Pool<K, C>
where
    K: Hash + Eq + Send + 'static,
    C: Connection,
{
    /// A pool whose options are read from the environment variables that
    /// start with `prefix` on first use; see [`Options::from_environment`].
    pub const fn new(runtime: &'static IoRuntime, prefix: &'static str, defaults: Options) -> Self {
        Self {
            runtime,
            prefix,
            defaults,
            options: OnceLock::new(),
            buckets: OnceLock::new(),
        }
    }

    /// A pool with fixed options, for tests that need small limits.
    pub fn with_options(runtime: &'static IoRuntime, options: Options) -> Self {
        Self {
            options: OnceLock::from(Ok(options)),
            ..Self::new(runtime, "", options)
        }
    }

    pub fn options(&self) -> Result<Options, String> {
        self.options
            .get_or_init(|| Options::from_environment(self.prefix, self.defaults))
            .clone()
    }

    pub async fn runtime(&self) -> Result<Handle, String> {
        self.runtime.handle(self.options()?.io_threads).await
    }

    /// Borrows a connection for `key`: an idle one that passes its check, or
    /// a new one from `connect`, which is only awaited when needed. When the
    /// bucket is full this waits, in arrival order, for at most
    /// `Options::wait`.
    pub async fn acquire(
        &self,
        key: K,
        connect: impl Future<Output = Result<C, C::Error>> + Send + 'static,
    ) -> Result<Lease<C>, AcquireError<C::Error>> {
        let options = self.options().map_err(AcquireError::Settings)?;
        let runtime = self.start(options).await?;
        let bucket = self.bucket(key, options, &runtime).await;
        let (mut sender, receiver) = oneshot::channel();
        runtime.spawn(async move {
            // A request that is cancelled while it waits leaves the queue at once.
            let permit = tokio::select! {
                _ = sender.closed() => return,
                permit = bucket.capacity.clone().acquire_owned() => permit,
            };
            let permit = permit.expect("a bucket never closes its semaphore");
            // Cancelled from here on, the undelivered lease is dropped, which
            // resets and returns its connection like any other.
            let _ = sender.send(bucket.lend(permit, connect).await);
        });
        match timeout(options.wait, receiver).await {
            Err(_) => Err(AcquireError::Exhausted {
                limit: options.size,
            }),
            Ok(Err(lost)) => Err(AcquireError::Runtime(lost.to_string())),
            Ok(Ok(lent)) => lent.map_err(AcquireError::Connect),
        }
    }

    /// Opens a connection outside the pool: it counts against no limit, and
    /// it is closed instead of reset when its last handle is dropped.
    pub async fn connect_unpooled(
        &self,
        connect: impl Future<Output = Result<C, C::Error>> + Send + 'static,
    ) -> Result<Lease<C>, AcquireError<C::Error>> {
        let options = self.options().map_err(AcquireError::Settings)?;
        let runtime = self.start(options).await?;
        // The lease is made inside the task, so that a request cancelled
        // during the handshake still closes the new connection.
        runtime
            .spawn(async move { connect.await.map(|connection| Lease::new(connection, None)) })
            .await
            .map_err(|lost| AcquireError::Runtime(lost.to_string()))?
            .map_err(AcquireError::Connect)
    }

    /// Waits until no connection for `key` is being opened, lent, reset or
    /// closed. Tests use it to make reuse deterministic.
    pub async fn settled(&self, key: &K) {
        let bucket = match self.buckets.get() {
            Some(buckets) => buckets.lock().await.get(key).cloned(),
            None => None,
        };
        if let Some(bucket) = bucket {
            let all = u32::try_from(bucket.options.size).unwrap_or(u32::MAX);
            let _all = bucket.capacity.acquire_many(all).await;
        }
    }

    async fn start(&self, options: Options) -> Result<Handle, AcquireError<C::Error>> {
        self.runtime
            .handle(options.io_threads)
            .await
            .map_err(AcquireError::Runtime)
    }

    async fn bucket(&self, key: K, options: Options, runtime: &Handle) -> Arc<Bucket<C>> {
        let mut buckets = self.buckets.get_or_init(Mutex::default).lock().await;
        buckets
            .entry(key)
            .or_insert_with(|| Bucket::new(options, runtime.clone()))
            .clone()
    }
}
