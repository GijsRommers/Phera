//! Every `Redis` object borrows a socket from a process-wide pool:
//!
//! ```php
//! $redis = new Redis();
//! $redis->connect('127.0.0.1', 6379); // borrows a pooled socket
//! $redis->select(3);
//! unset($redis);                      // RESET in the background, then idle
//! ```
//!
//! `connect` and `pconnect` both borrow; a persistent id is part of the pool
//! key, so it keeps its own sockets. How the pool works, and how a command
//! travels to the I/O threads and back, is described in the `phera-io-pool`
//! crate. The limits are environment settings, read once:
//! `PHERA_REDIS_POOL_SIZE` (512), `PHERA_REDIS_POOL_IDLE_SECONDS` (60),
//! `PHERA_REDIS_POOL_LIFETIME_SECONDS` (1800), `PHERA_REDIS_POOL_WAIT_SECONDS`
//! (30) and `PHERA_REDIS_IO_THREADS` (2).

mod socket;

pub use socket::{Endpoint, Socket};

use std::future::Future;
use std::time::Duration;

use phera_io_pool::{AcquireError, IoRuntime, Options, Pool};
use redis::aio::ConnectionLike;
use redis::{Cmd, RedisError, RedisResult, Value};
use tokio::runtime::Handle;
use tokio::time::timeout;

static RUNTIME: IoRuntime = IoRuntime::new("phera-redis-io");

static POOL: Pool<Endpoint, Socket> = Pool::new(
    &RUNTIME,
    "PHERA_REDIS",
    Options {
        size: 512,
        idle: Duration::from_secs(60),
        lifetime: Duration::from_secs(1800),
        wait: Duration::from_secs(30),
        io_threads: 2,
    },
);

/// The I/O runtime, for the subscription sockets that never enter the pool.
pub async fn runtime() -> RedisResult<Handle> {
    POOL.runtime().await.map_err(error)
}

/// A failure that is not the server's, in the form the rest of the crate
/// reports: `RedisException` with this message.
pub fn error(message: impl ToString) -> RedisError {
    std::io::Error::other(message.to_string()).into()
}

/// A borrowed socket. The `Redis` object, and a transaction or pipeline in
/// progress on it, share one lease.
#[derive(Clone)]
pub struct Lease(phera_io_pool::Lease<Socket>);

impl Lease {
    /// Sends one command and waits for its reply, for at most `read_timeout`
    /// seconds.
    pub async fn query(&self, command: Cmd, read_timeout: f64) -> RedisResult<Value> {
        self.0
            .run(move |socket| {
                Box::pin(async move {
                    let reply =
                        within(read_timeout, socket.connection.req_packed_command(&command)).await;
                    socket.retire_on_io_error(reply)
                })
            })
            .await
            .map_err(error)?
    }

    /// Sends the commands of a pipeline together and waits for all replies.
    pub async fn pipeline(&self, commands: Vec<Cmd>, read_timeout: f64) -> RedisResult<Vec<Value>> {
        if commands.is_empty() {
            return Ok(Vec::new());
        }
        self.0
            .run(move |socket| {
                Box::pin(async move {
                    let mut pipeline = redis::pipe();
                    for command in commands {
                        pipeline.add_command(command);
                    }
                    let count = pipeline.len();
                    let replies = within(
                        read_timeout,
                        socket.connection.req_packed_commands(&pipeline, 0, count),
                    )
                    .await;
                    socket.retire_on_io_error(replies)
                })
            })
            .await
            .map_err(error)?
    }

    /// Keeps the socket out of the pool once it is returned: after
    /// `rawCommand('SUBSCRIBE', ...)` or `MONITOR` it is no longer a plain
    /// request socket.
    pub async fn retire(&self) -> RedisResult<()> {
        self.0
            .run(|socket| Box::pin(async move { socket.reusable = false }))
            .await
            .map_err(error)
    }
}

/// phpredis's read timeout: at most `seconds` for a reply; 0 waits for ever.
async fn within<T>(seconds: f64, reply: impl Future<Output = RedisResult<T>>) -> RedisResult<T> {
    if seconds > 0.0 {
        timeout(Duration::from_secs_f64(seconds), reply)
            .await
            .map_err(error)?
    } else {
        reply.await
    }
}

pub async fn acquire(endpoint: Endpoint) -> RedisResult<Lease> {
    acquire_from(&POOL, endpoint).await
}

async fn acquire_from(pool: &Pool<Endpoint, Socket>, endpoint: Endpoint) -> RedisResult<Lease> {
    let connect = Socket::connect(endpoint.clone());
    pool.acquire(endpoint, connect)
        .await
        .map(Lease)
        .map_err(|failure| match failure {
            AcquireError::Settings(message) | AcquireError::Runtime(message) => error(message),
            AcquireError::Exhausted { limit } => {
                error(format!("Redis pool exhausted (limit {limit})"))
            }
            AcquireError::Connect(failure) => failure,
        })
}

#[cfg(test)]
mod tests;
