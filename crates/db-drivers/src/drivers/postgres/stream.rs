//! Passively observe PostgreSQL ReadyForQuery status; never rewrite protocol bytes.
//! This sees raw SQL, implicit transactions and failed transactions alike.

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub(super) trait Socket: AsyncRead + AsyncWrite + Send + Sync + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Sync + Unpin> Socket for T {}

pub struct ObservedStream {
    socket: Box<dyn Socket>,
    parser: Parser,
}

impl ObservedStream {
    pub(super) fn new(socket: Box<dyn Socket>, status: Arc<AtomicU8>) -> Self {
        Self {
            socket,
            parser: Parser::new(status),
        }
    }
}

enum Frame {
    Header { bytes: [u8; 5], used: usize },
    Body { kind: u8, remaining: usize },
}

struct Parser {
    frame: Frame,
    status: Arc<AtomicU8>,
}

impl Parser {
    fn new(status: Arc<AtomicU8>) -> Self {
        Self {
            frame: Self::header(),
            status,
        }
    }

    fn header() -> Frame {
        Frame::Header {
            bytes: [0; 5],
            used: 0,
        }
    }

    fn observe(&mut self, mut input: &[u8]) {
        while !input.is_empty() {
            match &mut self.frame {
                Frame::Header { bytes, used } => {
                    let length = (5 - *used).min(input.len());
                    bytes[*used..*used + length].copy_from_slice(&input[..length]);
                    *used += length;
                    input = &input[length..];
                    if *used == 5 {
                        let remaining = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]])
                            .saturating_sub(4) as usize;
                        self.frame = if remaining == 0 {
                            Self::header()
                        } else {
                            Frame::Body {
                                kind: bytes[0],
                                remaining,
                            }
                        };
                    }
                }
                Frame::Body { kind, remaining } => {
                    let length = (*remaining).min(input.len());
                    if *kind == b'Z' && *remaining == 1 {
                        self.status.store(input[0], Ordering::Release);
                    }
                    *remaining -= length;
                    input = &input[length..];
                    if *remaining == 0 {
                        self.frame = Self::header();
                    }
                }
            }
        }
    }
}

impl AsyncRead for ObservedStream {
    fn poll_read(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let start = output.filled().len();
        let result = Pin::new(&mut this.socket).poll_read(context, output);
        if let Poll::Ready(Ok(())) = &result {
            this.parser.observe(&output.filled()[start..]);
        }
        result
    }
}

impl AsyncWrite for ObservedStream {
    fn poll_write(
        self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().socket).poll_write(context, bytes)
    }

    fn poll_flush(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().socket).poll_flush(context)
    }

    fn poll_shutdown(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().socket).poll_shutdown(context)
    }
}

#[cfg(test)]
mod tests {
    use super::{Arc, AtomicU8, Ordering, Parser};

    #[test]
    fn observes_fragmented_frames_and_ignores_payload_lookalikes() {
        let status = Arc::new(AtomicU8::new(b'I'));
        let mut parser = Parser::new(status.clone());
        let packet = b"D\0\0\0\x0aZ\0\0\0\x05TZ\0\0\0\x05E";
        for byte in packet {
            parser.observe(&[*byte]);
        }
        assert_eq!(status.load(Ordering::Acquire), b'E');
        parser.observe(b"Z\0\0\0\x05TZ\0\0\0\x05I");
        assert_eq!(status.load(Ordering::Acquire), b'I');
    }
}
