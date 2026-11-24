//! A web request as the runtime sees it, and the response it produces.
//!
//! These types know nothing about the HTTP server: the command-line program
//! converts between the server's types and these, so the runtime can be used
//! and tested without a socket.

mod form_limits;
mod multipart;
mod query;
mod request_parse_body;
mod superglobals;
mod uploads;
pub(crate) mod web_output;

use std::net::SocketAddr;

pub(crate) use multipart::Boundary;
pub(crate) use request_parse_body::{is_urlencoded, register_request_parse_body};
pub(crate) use superglobals::{define_boot_superglobals, define_superglobals};
pub(crate) use uploads::Form;

fn media_type(content_type: &[u8]) -> &[u8] {
    content_type
        .split(|byte| matches!(byte, b';' | b',' | b' '))
        .next()
        .unwrap_or_default()
}

pub(crate) fn is_multipart(content_type: &[u8]) -> bool {
    media_type(content_type).eq_ignore_ascii_case(b"multipart/form-data")
}

pub(crate) async fn receive_form(body: Vec<u8>, boundary: Vec<u8>) -> Option<Form> {
    let range = 0..body.len();
    uploads::receive(body, range, boundary, form_limits::FormLimits::default())
        .await
        .ok()
}

/// Only POST fills PHP's startup form variables.
pub(crate) fn form_boundary(request: &HttpRequest) -> Boundary {
    request
        .header("Content-Type")
        .filter(|_| request.method == "POST")
        .map_or(Boundary::OtherMediaType, multipart::boundary)
}

/// One HTTP request, already read in full.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    /// Path and query as sent: `/blog/post.php?id=3`.
    pub target: String,
    /// The path alone, still percent-encoded: `/blog/post.php`.
    pub path: String,
    /// The query string without the `?`; empty if there is none.
    pub query: String,
    /// `HTTP/1.1`.
    pub protocol: String,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
    pub remote_addr: SocketAddr,
    pub local_addr: SocketAddr,
    /// The client reached the site over HTTPS, as far as the server knows:
    /// a trusted proxy in front said so. Scripts see it as
    /// `$_SERVER['HTTPS'] === 'on'`.
    pub https: bool,
}

impl HttpRequest {
    /// The first header with this name, compared without regard to case.
    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_slice())
    }
}

/// What goes back to the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
    pub websocket: Option<phera_extension_api::websocket::WebSocketAccept>,
}

impl HttpResponse {
    /// A response the runtime writes itself, such as "404 Not Found".
    pub(crate) fn plain(status: u16, body: impl Into<Vec<u8>>) -> Self {
        HttpResponse {
            status,
            headers: vec![(
                "Content-Type".to_string(),
                b"text/plain; charset=UTF-8".to_vec(),
            )],
            body: body.into(),
            websocket: None,
        }
    }

    /// The first header with this name, compared without regard to case.
    pub fn header(&self, name: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|(existing, _)| existing.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_slice())
    }
}
