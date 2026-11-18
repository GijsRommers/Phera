//! The body of a response on the wire: bytes from memory, or part of a
//! file read a chunk at a time.
//!
//! A 2 GB video is sent as 64 KiB pieces: the next piece is read from disk
//! only when the connection is ready to take it, so memory use does not
//! depend on the file's size or on how slowly the client reads.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use http_body_util::Full;
use hyper::body::{Body as HttpBody, Bytes, Frame, SizeHint};
use tokio::fs::File;
use tokio::io::{AsyncRead, ReadBuf};

use crate::response::Body;

const CHUNK_BYTES: u64 = 64 * 1024;

/// What hyper sends after the response headers.
pub(crate) enum WireBody {
    Memory(Full<Bytes>),
    File(FileBody),
}

impl From<Body> for WireBody {
    fn from(body: Body) -> Self {
        match body {
            Body::Bytes(bytes) => WireBody::Memory(Full::new(Bytes::from(bytes))),
            Body::File(slice) => WireBody::File(FileBody {
                file: File::from_std(slice.file),
                remaining: slice.length,
            }),
        }
    }
}

/// The rest of a file that is being sent.
pub(crate) struct FileBody {
    file: File,
    remaining: u64,
}

impl FileBody {
    /// Reads the next piece, of at most [`CHUNK_BYTES`].
    fn poll_chunk(&mut self, context: &mut Context<'_>) -> Poll<Option<io::Result<Bytes>>> {
        if self.remaining == 0 {
            return Poll::Ready(None);
        }
        let mut chunk = vec![0; self.remaining.min(CHUNK_BYTES) as usize];
        let mut buffer = ReadBuf::new(&mut chunk);
        let read = match Pin::new(&mut self.file).poll_read(context, &mut buffer) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(Err(error)) => return Poll::Ready(Some(Err(error))),
            Poll::Ready(Ok(())) => buffer.filled().len(),
        };
        if read == 0 {
            // The headers promised more bytes than the file still has: it
            // was truncated meanwhile. Failing closes the connection, which
            // is how the client learns the download is incomplete.
            let shorter = io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the file became shorter while it was being sent",
            );
            return Poll::Ready(Some(Err(shorter)));
        }
        chunk.truncate(read);
        self.remaining -= read as u64;
        Poll::Ready(Some(Ok(Bytes::from(chunk))))
    }
}

impl HttpBody for WireBody {
    type Data = Bytes;
    type Error = io::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, io::Error>>> {
        match self.get_mut() {
            WireBody::Memory(bytes) => Pin::new(bytes)
                .poll_frame(context)
                .map_err(|never| match never {}),
            WireBody::File(file) => file
                .poll_chunk(context)
                .map(|chunk| chunk.map(|bytes| bytes.map(Frame::data))),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            WireBody::Memory(bytes) => bytes.is_end_stream(),
            WireBody::File(file) => file.remaining == 0,
        }
    }

    /// The exact length, from which hyper writes `Content-Length`.
    fn size_hint(&self) -> SizeHint {
        match self {
            WireBody::Memory(bytes) => bytes.size_hint(),
            WireBody::File(file) => SizeHint::with_exact(file.remaining),
        }
    }
}
