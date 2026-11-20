//! The HTTP response a handler returns.

use std::fs::File;

use crate::StreamBody;

/// An HTTP response. Repeated header names (Set-Cookie) are allowed and
/// sent in order.
#[derive(Debug, Default)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Body,
}

/// What a response sends after its headers.
#[derive(Debug)]
pub enum Body {
    /// Bytes already in memory, such as everything a PHP script printed.
    Bytes(Vec<u8>),
    /// Part of a file, read from disk a chunk at a time while it is sent,
    /// so that a large download never sits in memory.
    File(FileSlice),
    /// Chunks produced while a handler is still running.
    Stream(StreamBody),
}

impl Default for Body {
    fn default() -> Self {
        Body::Bytes(Vec::new())
    }
}

/// The next `length` bytes of an open file.
#[derive(Debug)]
pub struct FileSlice {
    /// Positioned at the first byte to send.
    pub file: File,
    pub length: u64,
}

impl Response {
    /// An empty response with the given status.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: Body::default(),
        }
    }

    /// Add a header. The name is sent as given, so use lower-case for a
    /// predictable wire format.
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<Vec<u8>>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }

    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Body::Bytes(body.into());
        self
    }

    /// Send `length` bytes of `file`, starting where it is positioned.
    pub fn with_file(mut self, file: File, length: u64) -> Self {
        self.body = Body::File(FileSlice { file, length });
        self
    }

    /// Build a `text/plain` response, the everyday reply of a hello world.
    pub fn text(status: u16, body: &str) -> Self {
        Self::new(status)
            .with_header("content-type", "text/plain; charset=utf-8")
            .with_body(body.as_bytes().to_vec())
    }

    /// The first value of a header, matched case-insensitively.
    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.header_values(name).next()
    }

    /// Every value of a header, in order.
    pub fn header_values(&self, name: &str) -> impl Iterator<Item = &[u8]> {
        self.headers
            .iter()
            .filter(move |(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_slice())
    }
}
