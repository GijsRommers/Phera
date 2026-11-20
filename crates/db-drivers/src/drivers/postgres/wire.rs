//! Framing for the PostgreSQL socket owned by the PHP worker.

use std::io;
use std::task::Poll;

use bytes::BytesMut;
use fallible_iterator::FallibleIterator;
use postgres_protocol::message::backend::{ErrorFields, Header, Message};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::stream::Socket;
use crate::errors::DbError;

mod client;
pub(super) mod rows;
pub(super) mod startup;
pub(super) use client::Client;
pub use client::Notification;

#[cfg(test)]
mod scripted;

pub(super) const MAX_MESSAGE: usize = 64 * 1024 * 1024;
const READ_CHUNK: usize = 16 * 1024;

pub(super) struct Transport {
    socket: Option<Box<dyn Socket>>,
    input: BytesMut,
    output: BytesMut,
}

impl Transport {
    pub(super) fn new(socket: Box<dyn Socket>) -> Self {
        Self {
            socket: Some(socket),
            input: BytesMut::with_capacity(READ_CHUNK),
            output: BytesMut::new(),
        }
    }

    pub(super) fn is_broken(&self) -> bool {
        self.socket.is_none()
    }

    fn retire(&mut self) {
        self.socket = None;
    }

    pub(super) async fn send(
        &mut self,
        encode: impl FnOnce(&mut BytesMut) -> io::Result<()>,
    ) -> io::Result<()> {
        self.output.clear();
        encode(&mut self.output)?;
        let result = async {
            let socket = self.socket.as_mut().ok_or_else(closed)?;
            socket.write_all(&self.output).await?;
            socket.flush().await
        }
        .await;
        if result.is_err() {
            self.retire();
        }
        result
    }

    pub(super) async fn receive(&mut self) -> io::Result<Message> {
        let result = self.read_frame().await;
        if result.is_err() {
            self.retire();
        }
        result
    }

    /// Process already available data without waiting for a notification.
    async fn try_receive(&mut self) -> io::Result<Option<Message>> {
        let result = {
            let mut receive = std::pin::pin!(self.read_frame());
            std::future::poll_fn(|cx| {
                Poll::Ready(match receive.as_mut().poll(cx) {
                    Poll::Ready(result) => result.map(Some),
                    Poll::Pending => Ok(None),
                })
            })
            .await
        };
        if result.is_err() {
            self.retire();
        }
        result
    }

    async fn read_frame(&mut self) -> io::Result<Message> {
        loop {
            if let Some(message) = complete_frame(&mut self.input)? {
                return Ok(message);
            }
            self.input.reserve(READ_CHUNK);
            let read = self
                .socket
                .as_mut()
                .ok_or_else(closed)?
                .take(READ_CHUNK as u64)
                .read_buf(&mut self.input)
                .await?;
            if read == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "PostgreSQL closed the connection",
                ));
            }
        }
    }
}

fn complete_frame(input: &mut BytesMut) -> io::Result<Option<Message>> {
    let Some(header) = Header::parse(input)? else {
        return Ok(None);
    };
    let length = header.len() as usize;
    if length > MAX_MESSAGE {
        return Err(invalid("PostgreSQL message exceeds the 64 MiB limit"));
    }
    let Some(frame) = input.get(..length + 1) else {
        return Ok(None);
    };
    // These counts are used to reserve result or metadata storage. Check that
    // every advertised item could fit before handing the frame to the parser.
    let layout = match header.tag() {
        b'D' | b't' => Some((5, 4)),
        b'T' => Some((5, 19)),
        b'G' | b'H' => Some((6, 2)),
        _ => None,
    };
    if let Some((start, width)) = layout {
        let count = frame
            .get(start..start + 2)
            .ok_or_else(|| invalid("missing PostgreSQL item count"))?;
        let count = usize::from(u16::from_be_bytes([count[0], count[1]]));
        if count > (frame.len() - start - 2) / width {
            return Err(invalid("invalid PostgreSQL item count"));
        }
    }
    Message::parse(input)
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn closed() -> io::Error {
    io::Error::new(
        io::ErrorKind::NotConnected,
        "PostgreSQL connection is closed",
    )
}

fn ready_status(status: u8) -> io::Result<u8> {
    match status {
        b'I' | b'T' | b'E' => Ok(status),
        _ => Err(invalid("invalid PostgreSQL transaction status")),
    }
}

fn server_error(
    mut fields: ErrorFields<'_>,
    query: &str,
    include_context: bool,
) -> io::Result<DbError> {
    let mut severity = "ERROR".to_owned();
    let mut state = "XX000".to_owned();
    let mut message = "PostgreSQL reported an error".to_owned();
    let mut position = None;
    let mut detail = None;
    let mut hint = None;
    let mut context = None;
    while let Some(field) = fields.next()? {
        let value = || String::from_utf8_lossy(field.value_bytes());
        match field.type_() {
            b'S' => severity = value().into_owned(),
            b'C' => state = value().into_owned(),
            b'M' => message = value().into_owned(),
            b'P' => position = value().parse::<usize>().ok(),
            b'D' => detail = Some(value().into_owned()),
            b'H' => hint = Some(value().into_owned()),
            b'W' if include_context => context = Some(value().into_owned()),
            _ => {}
        }
    }
    let mut text = format!("{severity}:  {message}");
    if let Some(position) = position {
        let before: String = query.chars().take(position.saturating_sub(1)).collect();
        let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = before
            .rsplit('\n')
            .next()
            .unwrap_or_default()
            .chars()
            .count();
        let source = query.lines().nth(line - 1).unwrap_or_default();
        let label = format!("LINE {line}: ");
        text.push_str(&format!(
            "\n{label}{source}\n{}^",
            " ".repeat(label.len() + column)
        ));
    }
    for (label, value) in [("DETAIL", detail), ("HINT", hint), ("CONTEXT", context)] {
        if let Some(value) = value {
            text.push_str(&format!("\n{label}:  {value}"));
        }
    }
    Ok(DbError {
        state,
        code: Some(7),
        detail: text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fragmented_and_coalesced_frames_keep_their_order() {
        let (socket, mut peer) = tokio::io::duplex(128);
        let mut transport = Transport::new(Box::new(socket));
        let server = async {
            peer.write_all(b"1\0\0").await.unwrap();
            tokio::task::yield_now().await;
            peer.write_all(b"\0\x042\0\0\0\x04Z\0\0\0\x05I")
                .await
                .unwrap();
        };
        let client = async {
            assert!(matches!(
                transport.receive().await,
                Ok(Message::ParseComplete)
            ));
            assert!(matches!(
                transport.receive().await,
                Ok(Message::BindComplete)
            ));
            let Ok(Message::ReadyForQuery(ready)) = transport.receive().await else {
                panic!("expected ReadyForQuery");
            };
            assert_eq!(ready.status(), b'I');
        };
        tokio::join!(server, client);
    }

    #[tokio::test]
    async fn oversized_and_truncated_frames_retire_the_socket() {
        for bytes in [b"D\x7f\xff\xff\xff".as_slice(), b"D\0\0\0\x0a\0"] {
            let (socket, mut peer) = tokio::io::duplex(128);
            peer.write_all(bytes).await.unwrap();
            drop(peer);
            let mut transport = Transport::new(Box::new(socket));
            assert!(transport.receive().await.is_err());
            assert!(transport.is_broken());
        }
    }

    #[test]
    fn field_counts_must_fit_the_frame() {
        let mut input = BytesMut::from(&b"D\0\0\0\x06\xff\xff"[..]);
        assert!(complete_frame(&mut input).is_err());
        assert!(!input.is_empty());
    }
}
