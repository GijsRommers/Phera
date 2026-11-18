//! A small regular-file read such as `fread($f, 128)` runs on the worker.
//! Large transfers and descriptors that can wait yield to other requests.

use phera_extension_api::NativeError;

/// The cost of switching threads dominates small regular-file transfers.
/// Metadata lookups are also direct. Unknown sizes and special files can wait.
pub const INLINE_LIMIT: u64 = 64 * 1024;
pub const CHUNK: usize = 8192;

#[derive(Clone, Copy)]
pub enum IoPolicy {
    Inline,
    Blocking,
}

impl IoPolicy {
    pub fn transfer(regular: bool, length: Option<u64>) -> Self {
        if regular && length.is_some_and(|length| length <= INLINE_LIMIT) {
            Self::Inline
        } else {
            Self::Blocking
        }
    }

    pub async fn run<T: Send + 'static>(
        self,
        work: impl FnOnce() -> T + Send + 'static,
    ) -> Result<T, NativeError> {
        match self {
            Self::Inline => Ok(work()),
            Self::Blocking => tokio::task::spawn_blocking(work)
                .await
                .map_err(|error| NativeError::error(format!("Stream I/O task failed: {error}"))),
        }
    }
}
