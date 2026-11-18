//! The I/O runtime: a multi-threaded Tokio runtime, started on first use,
//! that owns the pooled connections for the rest of the process. A PHP worker
//! only hands it work and awaits the result, so `$redis->blPop($keys, 30)`
//! waits there while the worker runs other requests.

use std::sync::OnceLock;

use tokio::runtime::{Builder, Handle, Runtime};

pub struct IoRuntime {
    thread_name: &'static str,
    runtime: OnceLock<Result<Runtime, String>>,
}

impl IoRuntime {
    /// `thread_name` names its threads in a debugger or `top -H`.
    pub const fn new(thread_name: &'static str) -> Self {
        Self {
            thread_name,
            runtime: OnceLock::new(),
        }
    }

    /// The runtime, started with `threads` threads by the first call.
    ///
    /// Starting a runtime creates OS threads, which blocks, so the first call
    /// does it on Tokio's blocking pool rather than on the PHP worker.
    pub async fn handle(&'static self, threads: usize) -> Result<Handle, String> {
        if let Some(started) = self.runtime.get() {
            return handle(started);
        }
        tokio::task::spawn_blocking(move || {
            handle(self.runtime.get_or_init(|| {
                Builder::new_multi_thread()
                    .worker_threads(threads)
                    .thread_name(self.thread_name)
                    .enable_all()
                    .build()
                    .map_err(|error| error.to_string())
            }))
        })
        .await
        .map_err(|error| error.to_string())?
    }
}

fn handle(started: &Result<Runtime, String>) -> Result<Handle, String> {
    started
        .as_ref()
        .map(|runtime| runtime.handle().clone())
        .map_err(Clone::clone)
}
