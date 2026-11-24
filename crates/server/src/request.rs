//! The HTTP request handed to handlers.

use std::net::SocketAddr;

/// The HTTP protocol version of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpVersion {
    Http10,
    Http11,
    Http2,
}

/// A fully buffered HTTP request.
///
/// Plain owned data, no hyper types: handlers see exactly what arrived.
/// Header names are lower-cased, order and repetitions (Set-Cookie) are kept,
/// and path and query are the raw, percent-encoded bytes the client sent.
#[derive(Debug, Clone)]
pub struct Request {
    pub method: String,
    /// The raw request target, e.g. `/a/b?x=1`.
    pub target: String,
    /// The raw path part of the target, not percent-decoded, e.g. `/a/b`.
    pub path: String,
    /// The raw query without the `?`, empty when there is none.
    pub query: String,
    pub version: HttpVersion,
    /// Header names lower-cased, values raw bytes, order preserved.
    pub headers: Vec<(String, Vec<u8>)>,
    /// The fully buffered body, at most `ServerConfig::max_body_bytes` bytes.
    pub body: Vec<u8>,
    pub remote_addr: SocketAddr,
    pub local_addr: SocketAddr,
    /// True when the request asks for a WebSocket upgrade (`Upgrade: websocket`).
    pub is_websocket_upgrade: bool,
}

impl Request {
    /// The first value of a header, matched case-insensitively.
    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.header_values(name).next()
    }

    /// Every value of a header, in the order the client sent them.
    pub fn header_values(&self, name: &str) -> impl Iterator<Item = &[u8]> {
        self.headers
            .iter()
            .filter(move |(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_slice())
    }
}
