//! Common socket type for TCP, TLS and Unix-domain streams.

use tokio::io::{AsyncRead, AsyncWrite};

pub(super) trait Socket: AsyncRead + AsyncWrite + Send + Sync + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin> Socket for T {}
