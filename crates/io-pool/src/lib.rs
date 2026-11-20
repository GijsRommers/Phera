//! Redis connections pooled across workers and TLS settings shared with SQL drivers.
//!
//! In PHP a connection belongs to the request that opened it. In Phera many
//! requests share a few worker threads (docs/architecture.md, section 2), so a
//! PHP object borrows a pooled connection for as long as it lives, and the
//! connection is reset and lent again after the last handle is gone:
//!
//! ```php
//! $redis->pconnect($host); // borrows a pooled connection
//! $redis->get('cart');     // runs on an I/O thread
//! unset($redis);           // reset in the background, then idle
//! ```
//!
//! # How a query travels
//!
//! PHP values are `Rc`, so they never leave the worker thread they were made
//! on. Connections live on a separate multi-threaded Tokio runtime, the I/O
//! runtime, so a connection outlives the worker that opened it and can serve
//! requests from every worker. Only owned data that is `Send` crosses between
//! the two:
//!
//! ```text
//!  PHP worker thread                       I/O runtime ("phera-redis-io")
//!  -----------------                       ---------------------------------------------
//!  $redis->pconnect(...)
//!    find the bucket for this key
//!    spawn ------------------------------> wait for a permit, first come first served
//!                                          reuse an idle connection that passes its
//!                                            check, or open a new one
//!    await the reply, at most `wait`  <--- Lease
//!
//!  $redis->get('cart')
//!    owned SQL and parameters
//!    spawn ------------------------------> lock the lease's connection
//!    await the task; meanwhile the         send the query, await the rows
//!      worker runs other requests          unlock
//!    rows as plain Rust data          <--- task result
//!    convert them to PHP values
//!
//!  the last handle is dropped -----------> reset (at most 5 s), then idle or close;
//!                                          release the permit
//! ```
//!
//! A bucket holds the connections for one Redis endpoint and persistent id. Its
//! semaphore has `size` permits, and a permit is held while a connection is
//! being opened, lent, reset or closed, so the server never sees more than
//! `size` connections for one key, from all workers together.
//!
//! An operation's task keeps the lease alive until the operation finishes,
//! even when the PHP request that started it is cancelled: the connection is
//! never reset or lent again in the middle of a query.

mod bucket;
mod lease;
mod pool;
mod runtime;

use std::future::Future;
use std::time::Duration;

pub use lease::{Lease, Operation};
pub use pool::{AcquireError, Pool};
pub use runtime::IoRuntime;

/// What the pool needs from a connection. Opening one is up to the caller of
/// [`Pool::acquire`]; everything else the pool does by itself.
///
/// Each method runs on the I/O runtime, which is why its future must be `Send`.
pub trait Connection: Send + 'static {
    type Error: Send + 'static;

    /// Whether an idle connection may be lent again after `idle` time unused.
    /// A connection that fails is closed and the pool tries the next one.
    fn check(&mut self, idle: Duration) -> impl Future<Output = bool> + Send;

    /// Whether the connection may go back to the pool at all, for example
    /// `false` after an I/O error or once it holds state a reset cannot clear.
    fn reusable(&self) -> bool;

    /// Clears everything one PHP object may have left behind (an open
    /// transaction, session variables, a selected database) before the next
    /// one borrows the connection. A failed reset closes the connection.
    fn reset(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send;

    fn close(self) -> impl Future<Output = ()> + Send;
}

/// The limits of every pool of one extension. They are application settings,
/// read once from the environment; a PHP script cannot change them.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// The most connections one bucket has open at a time, over all workers.
    pub size: usize,
    /// How long a returned connection may stay idle before it is closed.
    pub idle: Duration,
    /// How old a connection may get; an older one is closed when returned.
    pub lifetime: Duration,
    /// How long a new PHP object waits for a connection when all are busy.
    pub wait: Duration,
    /// Threads in the I/O runtime.
    pub io_threads: usize,
}

impl Options {
    /// Reads `{prefix}_POOL_SIZE`, `{prefix}_POOL_IDLE_SECONDS`,
    /// `{prefix}_POOL_LIFETIME_SECONDS`, `{prefix}_POOL_WAIT_SECONDS` and
    /// `{prefix}_IO_THREADS`; a variable that is not set keeps its default.
    pub fn from_environment(prefix: &str, defaults: Options) -> Result<Options, String> {
        let mut settings = Self::pool_limits_from_environment(prefix, defaults)?;
        settings.io_threads = number(prefix, "IO_THREADS", defaults.io_threads as u64)? as usize;
        Ok(settings)
    }

    /// Reads the pool limits for a caller that supplies its own event loop.
    pub fn pool_limits_from_environment(
        prefix: &str,
        defaults: Options,
    ) -> Result<Options, String> {
        let number = |suffix, default| number(prefix, suffix, default);
        let seconds = |suffix: &str, default: Duration| {
            number(suffix, default.as_secs()).map(Duration::from_secs)
        };
        Ok(Options {
            size: number("POOL_SIZE", defaults.size as u64)? as usize,
            idle: seconds("POOL_IDLE_SECONDS", defaults.idle)?,
            lifetime: seconds("POOL_LIFETIME_SECONDS", defaults.lifetime)?,
            wait: seconds("POOL_WAIT_SECONDS", defaults.wait)?,
            io_threads: defaults.io_threads,
        })
    }
}

fn number(prefix: &str, suffix: &str, default: u64) -> Result<u64, String> {
    let name = format!("{prefix}_{suffix}");
    match std::env::var(&name) {
        Err(std::env::VarError::NotPresent) => Ok(default),
        value => value
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|value| (1..=u64::from(u32::MAX)).contains(value))
            .ok_or_else(|| {
                format!(
                    "{name} must be a positive integer no greater than {}",
                    u32::MAX
                )
            }),
    }
}

#[cfg(test)]
mod tests;
pub mod tls;
