//! `$r->connect(...)` exclusively borrows a socket owned by the process I/O runtime.
//! Dropping a cancelled request retains its lease until the outstanding command finishes.

mod settings;
mod socket;

use phera_io_pool::Bucket;
use redis::aio::ConnectionLike;
use redis::{Cmd, RedisError, RedisResult, Value};
pub use socket::Endpoint;
use socket::Socket;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::{Mutex, OwnedSemaphorePermit, oneshot};
use tokio::time::{Instant, timeout};

static RUNTIME: phera_io_pool::IoRuntime = phera_io_pool::IoRuntime::new();
static POOLS: OnceLock<Mutex<HashMap<Endpoint, Arc<Bucket<Socket>>>>> = OnceLock::new();

pub async fn runtime() -> RedisResult<tokio::runtime::Handle> {
    RUNTIME
        .handle(settings::application()?.io_threads, "phera-redis-io")
        .await
        .map_err(error)
}

pub fn error(message: impl ToString) -> RedisError {
    std::io::Error::other(message.to_string()).into()
}

struct ReturnTo {
    bucket: Arc<Bucket<Socket>>,
    permit: OwnedSemaphorePermit,
    born: Instant,
}

struct Shared {
    socket: Mutex<Option<Socket>>,
    owner: Option<ReturnTo>,
    runtime: tokio::runtime::Handle,
}

impl Drop for Shared {
    fn drop(&mut self) {
        if let Some(socket) = self.socket.get_mut().take() {
            let owner = self.owner.take();
            self.runtime.spawn(async move {
                if let Some(owner) = owner {
                    owner.bucket.recycle(socket, owner.born, owner.permit).await;
                }
            });
        }
    }
}

#[derive(Clone)]
pub struct Lease(Arc<Shared>);

impl Lease {
    pub async fn query(&self, command: Cmd, read_timeout: f64) -> RedisResult<Value> {
        let shared = self.0.clone();
        self.0
            .runtime
            .spawn(async move {
                let mut guard = shared.socket.lock().await;
                let socket = guard.as_mut().expect("live lease");
                let future = socket.connection.req_packed_command(&command);
                let result = if read_timeout > 0.0 {
                    timeout(std::time::Duration::from_secs_f64(read_timeout), future)
                        .await
                        .map_err(error)
                        .and_then(|result| result)
                } else {
                    future.await
                };
                if result.as_ref().is_err_and(|e: &RedisError| e.is_io_error()) {
                    socket.reusable = false;
                }
                result
            })
            .await
            .map_err(error)?
    }

    pub async fn pipeline(&self, commands: Vec<Cmd>, read_timeout: f64) -> RedisResult<Vec<Value>> {
        if commands.is_empty() {
            return Ok(Vec::new());
        }
        let shared = self.0.clone();
        self.0
            .runtime
            .spawn(async move {
                let mut guard = shared.socket.lock().await;
                let socket = guard.as_mut().expect("live lease");
                let mut pipeline = redis::pipe();
                for command in commands {
                    pipeline.add_command(command);
                }
                let count = pipeline.len();
                let future = socket.connection.req_packed_commands(&pipeline, 0, count);
                let result = if read_timeout > 0.0 {
                    timeout(std::time::Duration::from_secs_f64(read_timeout), future)
                        .await
                        .map_err(error)
                        .and_then(|r| r)
                } else {
                    future.await
                };
                if result.as_ref().is_err_and(|e| e.is_io_error()) {
                    socket.reusable = false;
                }
                result
            })
            .await
            .map_err(error)?
    }

    pub async fn retire(&self) -> RedisResult<()> {
        let shared = self.0.clone();
        self.0
            .runtime
            .spawn(async move {
                shared
                    .socket
                    .lock()
                    .await
                    .as_mut()
                    .expect("live lease")
                    .reusable = false;
            })
            .await
            .map_err(error)
    }
}

pub async fn acquire(endpoint: Endpoint) -> RedisResult<Lease> {
    let options = settings::application()?;
    let handle = runtime().await?;
    let (mut sender, receiver) = oneshot::channel();
    handle.spawn(async move {
        let bucket = POOLS
            .get_or_init(Mutex::default)
            .lock()
            .await
            .entry(endpoint.clone())
            .or_insert_with(|| Bucket::new(options))
            .clone();
        let permit = tokio::select! {
            _ = sender.closed() => return,
            permit = bucket.capacity.clone().acquire_owned() => permit,
        };
        let result = match permit {
            Ok(permit) => borrow(bucket, permit, endpoint).await,
            Err(e) => Err(error(e)),
        };
        let _ = sender.send(result);
    });
    timeout(options.wait, receiver)
        .await
        .map_err(|_| error(format!("Redis pool exhausted (limit {})", options.size)))?
        .map_err(error)?
}

async fn borrow(
    bucket: Arc<Bucket<Socket>>,
    permit: OwnedSemaphorePermit,
    endpoint: Endpoint,
) -> RedisResult<Lease> {
    let mut selected = None;
    while let Some(mut idle) = bucket.take_idle().await {
        if !bucket.expired(&idle)
            && matches!(
                timeout(
                    std::time::Duration::from_secs(5),
                    redis::cmd("PING").query_async::<Value>(&mut idle.driver.connection)
                )
                .await,
                Ok(Ok(Value::SimpleString(_)))
            )
        {
            selected = Some((idle.driver, idle.born));
            break;
        }
    }
    let (socket, born) = match selected {
        Some(connection) => connection,
        None => (Socket::connect(endpoint).await?, Instant::now()),
    };
    Ok(Lease(Arc::new(Shared {
        socket: Mutex::new(Some(socket)),
        owner: Some(ReturnTo {
            bucket,
            permit,
            born,
        }),
        runtime: tokio::runtime::Handle::current(),
    })))
}

#[cfg(test)]
mod tests;
