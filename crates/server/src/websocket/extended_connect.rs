//! WebSockets over HTTP/2: the extended CONNECT of RFC 8441.
//!
//! A client that already has an HTTP/2 connection opens a WebSocket as one
//! more stream on it. The server announces `SETTINGS_ENABLE_CONNECT_PROTOCOL
//! = 1`, the client sends
//!
//! ```text
//! :method CONNECT   :protocol websocket   :path /chat   sec-websocket-version 13
//! ```
//!
//! and a `200` answer turns the stream into a WebSocket: its DATA frames carry
//! the same RFC 6455 frames as an upgraded HTTP/1.1 socket. PHP sees the
//! request exactly as it sees the HTTP/1.1 upgrade, apart from
//! `SERVER_PROTOCOL`:
//!
//! ```php
//! // $_SERVER['REQUEST_METHOD'] === 'GET', $_SERVER['HTTP_UPGRADE'] === 'websocket'
//! // $_SERVER['SERVER_PROTOCOL'] === 'HTTP/2.0'
//! Phera\WebSocket\accept(Chat::class);
//! ```

use hyper::ext::Protocol;
use hyper::header::HeaderMap;
use hyper::{Method, Request as HyperRequest};

use crate::request::Request;
use crate::response::Response;

use super::handshake::{
    UpgradeRejection, has_supported_version, keep_application_headers, push_negotiated_subprotocol,
};

/// What an HTTP/2 request asks for, as far as WebSockets are concerned.
pub(crate) enum Http2Connect {
    /// Any request that is not an extended CONNECT: served as before.
    Ordinary,
    /// `:method CONNECT` with `:protocol websocket`.
    WebSocket,
    /// An extended CONNECT for a protocol this server does not speak
    /// (WebTransport, `connect-udp`, ...).
    OtherProtocol,
}

/// Sort an HTTP/2 request. An ordinary request costs one method comparison.
#[inline]
pub(crate) fn classify<B>(request: &HyperRequest<B>) -> Http2Connect {
    if request.method() != Method::CONNECT {
        return Http2Connect::Ordinary;
    }
    // A CONNECT without `:protocol` is a classic proxy tunnel, which hyper
    // and the handler treat as they always did.
    match request.extensions().get::<Protocol>() {
        None => Http2Connect::Ordinary,
        // Upgrade tokens compare case-insensitively (RFC 9110 section 7.8).
        Some(protocol) if protocol.as_str().eq_ignore_ascii_case("websocket") => {
            Http2Connect::WebSocket
        }
        Some(_) => Http2Connect::OtherProtocol,
    }
}

/// RFC 8441 section 5: `Sec-WebSocket-Key` and `Sec-WebSocket-Accept` are not
/// used on HTTP/2; the version still has to be 13.
pub(crate) fn validate_handshake(headers: &HeaderMap) -> Result<(), UpgradeRejection> {
    if has_supported_version(headers) {
        Ok(())
    } else {
        Err(UpgradeRejection::UnsupportedVersion)
    }
}

/// Show PHP the request an HTTP/1.1 browser would have sent, so that a
/// router matching `GET /chat` and code checking `HTTP_UPGRADE` work
/// unchanged. `SERVER_PROTOCOL` stays `HTTP/2.0`, the one documented way to
/// tell the transports apart. HTTP/2 itself forbids both headers (RFC 9113
/// section 8.2.2), so the client cannot have sent them.
pub(crate) fn present_as_upgrade(request: &mut Request) {
    request.method = Method::GET.as_str().to_string();
    request
        .headers
        .push(("upgrade".to_string(), b"websocket".to_vec()));
    request
        .headers
        .push(("connection".to_string(), b"Upgrade".to_vec()));
}

/// The `200` that opens the WebSocket on the stream, keeping the headers PHP
/// set before `accept()` (`Set-Cookie`, ...). Hyper sends it without
/// END_STREAM and hands the stream over as an upgraded connection.
pub(crate) fn build_200_response(
    mut response: Response,
    subprotocol: Option<&str>,
    client_subprotocols: Option<&[u8]>,
) -> Response {
    response.status = 200;
    keep_application_headers(&mut response);
    push_negotiated_subprotocol(&mut response, subprotocol, client_subprotocols);
    response
}

/// The answer to an extended CONNECT for another protocol.
pub(crate) fn other_protocol_response() -> Response {
    Response::text(501, "only the websocket protocol is supported\n")
}
