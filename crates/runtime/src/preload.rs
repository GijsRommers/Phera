//! Composer preload compiles class-map entries and `autoload.files` before
//! a request can reach `require 'vendor/autoload.php'`.

mod composer;

use std::path::Path;
use std::time::{Duration, Instant};

use crate::{Runtime, RuntimeError};

/// Work performed before the listener was opened.
pub struct PreloadSummary {
    pub compiled: usize,
    pub skipped: usize,
    pub elapsed: Duration,
}

impl Runtime {
    /// Read Composer metadata as data, then compile its source files on
    /// a bounded pool. Blank or absent maps leave the cache empty.
    pub fn preload_composer(&self, project: &Path) -> Result<PreloadSummary, RuntimeError> {
        let start = Instant::now();
        let threads = std::thread::available_parallelism().map_or(1, |count| count.get().min(4));
        let executor = tokio::runtime::Builder::new_current_thread()
            .max_blocking_threads(threads)
            .thread_stack_size(phera_parser::PARSER_STACK_BYTES)
            .build()
            .map_err(internal)?;
        let project = project.to_path_buf();
        let (compiled, skipped) = executor.block_on(async {
            let (paths, unavailable) =
                tokio::task::spawn_blocking(move || composer::files(&project))
                    .await
                    .map_err(internal)?;
            let mut groups = vec![Vec::new(); threads];
            for (index, path) in paths.into_iter().enumerate() {
                groups[index % threads].push(path);
            }

            let mut jobs = tokio::task::JoinSet::new();
            for group in groups.into_iter().filter(|group| !group.is_empty()) {
                let cache = self.web_cache.clone();
                jobs.spawn_blocking(move || {
                    let count = group.len();
                    let compiled = group
                        .into_iter()
                        .filter(|path| cache.preload_file(path).is_ok())
                        .count();
                    (compiled, count - compiled)
                });
            }
            let mut compiled = 0;
            let mut skipped = unavailable;
            while let Some(result) = jobs.join_next().await {
                let (success, failure) = result.map_err(internal)?;
                compiled += success;
                skipped += failure;
            }
            Ok::<_, RuntimeError>((compiled, skipped))
        })?;
        Ok(PreloadSummary {
            compiled,
            skipped,
            elapsed: start.elapsed(),
        })
    }
}

fn internal(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::Internal(error.to_string())
}
