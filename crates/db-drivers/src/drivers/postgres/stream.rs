//! Common socket type for TCP, TLS and Unix-domain streams.

use tokio::io::{AsyncRead, AsyncWrite};

#[cfg(any(target_os = "macos", all(test, target_os = "linux")))]
pub(super) mod read_interest;

pub(super) trait Socket: AsyncRead + AsyncWrite + Send + Sync + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin> Socket for T {}
