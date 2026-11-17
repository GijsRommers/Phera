//! This runtime lives for the process lifetime, independently of all PHP workers.

use std::future::Future;
use std::sync::OnceLock;

use tokio::runtime::{Builder, Handle, Runtime};

use crate::drivers::connection_error;
use crate::errors::DbError;

static RUNTIME: OnceLock<Result<Runtime, DbError>> = OnceLock::new();

fn initialized() -> Option<Result<Handle, DbError>> {
    RUNTIME.get().map(|runtime| {
        runtime
            .as_ref()
            .map(|runtime| runtime.handle().clone())
            .map_err(Clone::clone)
    })
}

pub fn initialized_handle() -> Option<Handle> {
    initialized().and_then(Result::ok)
}

pub async fn handle() -> Result<Handle, DbError> {
    if let Some(runtime) = initialized() {
        return runtime;
    }
    // Thread creation and concurrent OnceLock initialization may block. Do them
    // on Tokio's blocking executor, never on a PHP worker. Install the runtime
    // inside that task so cancellation cannot drop a Runtime on a PHP worker.
    tokio::task::spawn_blocking(|| {
        RUNTIME.get_or_init(|| {
            Builder::new_multi_thread()
                .worker_threads(super::options::application()?.io_threads)
                .thread_name("phera-db-io")
                .enable_all()
                .build()
                .map_err(connection_error)
        });
        initialized().expect("initialization completed")
    })
    .await
    .map_err(connection_error)?
}

pub async fn run<T: Send + 'static>(
    future: impl Future<Output = Result<T, DbError>> + Send + 'static,
) -> Result<T, DbError> {
    // Detaching on cancellation lets an in-flight protocol operation finish. Its
    // captured lease keeps the connection exclusive until it can safely be reset.
    handle()
        .await?
        .spawn(future)
        .await
        .map_err(connection_error)?
}
