//! The same listener accepts HTTP/1 and prior-knowledge HTTP/2. A typical
//! HTTP/1 request stays in the TCP buffer until Hyper reads it.

use std::io;

use tokio::io::{AsyncReadExt, ReadBuf};
use tokio::net::TcpStream;

const PREFACE: &[u8; 24] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

pub(crate) enum Protocol {
    Http1,
    Http2,
}

/// Bytes read while deciding whether a partial preface was HTTP/2.
pub(crate) struct ReadAhead {
    bytes: [u8; PREFACE.len()],
    start: usize,
    end: usize,
}

impl ReadAhead {
    pub(crate) fn copy_to(&mut self, buffer: &mut ReadBuf<'_>) -> bool {
        let length = (self.end - self.start).min(buffer.remaining());
        let end = self.start + length;
        buffer.put_slice(&self.bytes[self.start..end]);
        self.start = end;
        self.start == self.end
    }
}

pub(crate) async fn detect(stream: &mut TcpStream) -> io::Result<(Protocol, Option<ReadAhead>)> {
    let mut bytes = [0; PREFACE.len()];
    let seen = stream.peek(&mut bytes).await?;
    if seen == 0 || !PREFACE.starts_with(&bytes[..seen]) {
        return Ok((Protocol::Http1, None));
    }
    if seen == PREFACE.len() {
        return Ok((Protocol::Http2, None));
    }

    // A short peek could be the start of HTTP/2 (or an HTTP/1 method such
    // as POST, only its "P" having arrived). Consume and replay those bytes
    // so waiting for the rest does not repeatedly wake on the same prefix.
    let mut end = 0;
    let protocol = loop {
        let count = stream.read(&mut bytes[end..]).await?;
        end += count;
        if count == 0 || !PREFACE.starts_with(&bytes[..end]) {
            break Protocol::Http1;
        }
        if end == PREFACE.len() {
            break Protocol::Http2;
        }
    };
    Ok((
        protocol,
        Some(ReadAhead {
            bytes,
            start: 0,
            end,
        }),
    ))
}
