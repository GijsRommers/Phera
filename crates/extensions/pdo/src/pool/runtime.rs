//! PDO uses the shared process-lifetime I/O runtime host.

use crate::drivers::connection_error;
use crate::errors::DbError;
use std::future::Future;
use tokio::runtime::Handle;

static RUNTIME: phera_io_pool::IoRuntime = phera_io_pool::IoRuntime::new();

pub async fn handle() -> Result<Handle, DbError> {
    RUNTIME
        .handle(super::options::application()?.io_threads, "phera-db-io")
        .await
        .map_err(connection_error)
}

pub async fn run<T: Send + 'static>(
    future: impl Future<Output = Result<T, DbError>> + Send + 'static,
) -> Result<T, DbError> {
    handle()
        .await?
        .spawn(future)
        .await
        .map_err(connection_error)?
}
