//! Driver tasks outlive PHP workers: `$redis->blPop(...)` yields its request thread.

use std::sync::OnceLock;
use tokio::runtime::{Builder, Handle, Runtime};

pub struct IoRuntime(OnceLock<Result<Runtime, String>>);

impl Default for IoRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl IoRuntime {
    pub const fn new() -> Self {
        Self(OnceLock::new())
    }

    pub async fn handle(
        &'static self,
        threads: usize,
        name: &'static str,
    ) -> Result<Handle, String> {
        if let Some(runtime) = self.0.get() {
            return runtime
                .as_ref()
                .map(|r| r.handle().clone())
                .map_err(Clone::clone);
        }
        // Runtime construction creates OS threads. Never do that on a PHP worker.
        tokio::task::spawn_blocking(move || {
            self.0.get_or_init(|| {
                Builder::new_multi_thread()
                    .worker_threads(threads)
                    .thread_name(name)
                    .enable_all()
                    .build()
                    .map_err(|e| e.to_string())
            });
            self.0
                .get()
                .expect("initialized")
                .as_ref()
                .map(|r| r.handle().clone())
                .map_err(Clone::clone)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
