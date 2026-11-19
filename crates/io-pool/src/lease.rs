//! A borrowed connection. A PHP object and everything that keeps it alive
//! (PDO's statements, a Redis transaction) share one lease; when the last
//! clone is dropped, the connection goes back to its bucket.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::runtime::Handle;
use tokio::sync::{Mutex, OwnedSemaphorePermit};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::Connection;
use crate::bucket::Bucket;

/// The future an operation passed to [`Lease::run`] returns. It may borrow
/// the connection (the `'c`), and it must be `Send` to run on the I/O runtime.
pub type Operation<'c, T> = Pin<Box<dyn Future<Output = T> + Send + 'c>>;

pub struct Lease<C: Connection> {
    shared: Arc<Shared<C>>,
}

struct Shared<C: Connection> {
    /// Always `Some`: only `Drop` takes the connection out, to return it.
    connection: Mutex<Option<C>>,
    /// Where the connection goes back to. `None` for a connection opened
    /// with [`crate::Pool::connect_unpooled`], which is closed instead.
    owner: Option<Owner<C>>,
    /// The I/O runtime, where operations, reset and close run.
    runtime: Handle,
}

pub(crate) struct Owner<C: Connection> {
    bucket: Arc<Bucket<C>>,
    permit: OwnedSemaphorePermit,
    born: Instant,
}

impl<C: Connection> Owner<C> {
    pub(crate) fn new(bucket: Arc<Bucket<C>>, permit: OwnedSemaphorePermit, born: Instant) -> Self {
        Self {
            bucket,
            permit,
            born,
        }
    }
}

impl<C: Connection> Lease<C> {
    /// Must be called on the I/O runtime, which the lease remembers.
    pub(crate) fn new(connection: C, owner: Option<Owner<C>>) -> Self {
        Self {
            shared: Arc::new(Shared {
                connection: Mutex::new(Some(connection)),
                owner,
                runtime: Handle::current(),
            }),
        }
    }

    /// Reads the connection on the calling thread, without waiting, unless an
    /// operation is using it. PDO reads the server version of a new lease
    /// this way.
    pub fn try_inspect<T>(&self, inspect: impl FnOnce(&C) -> T) -> Option<T> {
        let connection = self.shared.connection.try_lock().ok()?;
        connection.as_ref().map(inspect)
    }

    /// Runs `operation` on the I/O runtime with exclusive use of the
    /// connection, and waits for its result without blocking the worker.
    ///
    /// The task holds its own clone of the lease, so when the caller stops
    /// waiting (a cancelled PHP request) the operation still finishes before
    /// the connection can be reset. The error says why the operation did not
    /// run to the end: the runtime is shutting down, or the task failed.
    pub async fn run<T, F>(&self, operation: F) -> Result<T, String>
    where
        T: Send + 'static,
        F: for<'c> FnOnce(&'c mut C) -> Operation<'c, T> + Send + 'static,
    {
        self.start(operation)
            .await
            .map_err(|lost| lost.to_string())?
    }

    /// Starts work immediately on the I/O runtime. Dropping the returned
    /// handle detaches the task, which still owns the lease until it finishes.
    pub fn start<T, F>(&self, operation: F) -> JoinHandle<Result<T, String>>
    where
        T: Send + 'static,
        F: for<'c> FnOnce(&'c mut C) -> Operation<'c, T> + Send + 'static,
    {
        let shared = self.shared.clone();
        self.shared.runtime.spawn(async move {
            let mut connection = shared.connection.lock().await;
            // `None` is only possible when the connection has been returned.
            let connection = connection
                .as_mut()
                .ok_or_else(|| "the connection was already returned to its pool".to_owned())?;
            Ok(operation(connection).await)
        })
    }
}

impl<C: Connection> Clone for Lease<C> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<C: Connection> Drop for Shared<C> {
    fn drop(&mut self) {
        let Some(connection) = self.connection.get_mut().take() else {
            return;
        };
        let owner = self.owner.take();
        self.runtime.spawn(async move {
            match owner {
                Some(owner) => {
                    let Owner {
                        bucket,
                        permit,
                        born,
                    } = owner;
                    bucket.recycle(connection, born, permit).await;
                }
                None => connection.close().await,
            }
        });
    }
}
