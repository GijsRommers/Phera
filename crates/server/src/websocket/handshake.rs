//! HTTP/1.1 WebSocket upgrade header and `Origin` validation (RFC 6455).

use hyper::header::{CONNECTION, HeaderMap, HeaderName, UPGRADE};
use hyper::{Method, Version};
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;

use crate::response::Response;

const SEC_WEBSOCKET_KEY: &str = "sec-websocket-key";
const SEC_WEBSOCKET_VERSION: &str = "sec-websocket-version";
const SEC_WEBSOCKET_PROTOCOL: &str = "sec-websocket-protocol";

/// Why an upgrade request was rejected before handing the socket to the stream loop.
#[derive(Debug, Clone, Copy)]
pub(crate) enum UpgradeRejection {
    BadRequest(&'static str),
    Forbidden,
    UpgradeRequired,
}

impl UpgradeRejection {
    pub(crate) fn into_response(self) -> Response {
        match self {
            Self::BadRequest(message) => Response::text(400, message),
            Self::Forbidden => Response::text(403, "origin not allowed\n"),
            Self::UpgradeRequired => Response::text(426, "Upgrade Required\n")
                .with_header("upgrade", "websocket")
                .with_header("connection", "close"),
        }
    }
}

/// Fast check before running handlers; false in two header checks for ordinary HTTP.
#[inline]
pub(crate) fn is_websocket_upgrade_request(
    method: &Method,
    version: Version,
    headers: &HeaderMap,
) -> bool {
    method == Method::GET
        && version == Version::HTTP_11
        && headers
            .get(UPGRADE)
            .is_some_and(|value| contains_token_ignore_ascii_case(value.as_bytes(), b"websocket"))
}

/// Validate RFC 6455 handshake headers and return the `Sec-WebSocket-Accept` value.
pub(crate) fn validate_handshake(headers: &HeaderMap) -> Result<String, UpgradeRejection> {
    let has_connection_upgrade = headers
        .get_all(CONNECTION)
        .iter()
        .any(|value| contains_token_ignore_ascii_case(value.as_bytes(), b"upgrade"));
    if !has_connection_upgrade {
        return Err(UpgradeRejection::BadRequest(
            "missing Connection: Upgrade header\n",
        ));
    }

    let version_ok = headers
        .get(HeaderName::from_static(SEC_WEBSOCKET_VERSION))
        .is_some_and(|value| value.as_bytes().trim_ascii() == b"13");
    if !version_ok {
        return Err(UpgradeRejection::UpgradeRequired);
    }

    let Some(key) = headers.get(HeaderName::from_static(SEC_WEBSOCKET_KEY)) else {
        return Err(UpgradeRejection::BadRequest(
            "missing Sec-WebSocket-Key header\n",
        ));
    };
    let key_bytes = key.as_bytes().trim_ascii();
    if !is_valid_websocket_key(key_bytes) {
        return Err(UpgradeRejection::BadRequest(
            "invalid Sec-WebSocket-Key header\n",
        ));
    }
    Ok(derive_accept_key(key_bytes))
}

/// Check whether a WebSocket `Origin` header is allowed under `allowed_origins`
/// (or same-origin by default when `allowed_origins` is `None`).
pub fn validate_origin(
    origin_header: Option<&[u8]>,
    effective_https: bool,
    effective_host: Option<&[u8]>,
    allowed_origins: Option<&[String]>,
) -> bool {
    let Some(raw_origin) = origin_header else {
        // Non-browser clients omit Origin unless an allowlist explicitly requires one.
        return allowed_origins
            .is_none_or(|list| list.is_empty() || list.iter().any(|item| item == "*"));
    };
    let Ok(origin) = std::str::from_utf8(raw_origin) else {
        return false;
    };
    let origin = origin.trim();
    if let Some(allowlist) = allowed_origins {
        return allowlist
            .iter()
            .any(|allowed| allowed == "*" || origins_equal(origin, allowed));
    }
    let Some((origin_scheme, origin_authority)) = split_origin(origin) else {
        return false;
    };
    let expected_scheme = if effective_https { "https" } else { "http" };
    let Some(raw_host) = effective_host else {
        return false;
    };
    let Ok(host) = std::str::from_utf8(raw_host) else {
        return false;
    };
    origin_scheme.eq_ignore_ascii_case(expected_scheme)
        && authorities_equal(origin_authority, host.trim(), expected_scheme)
}

/// Build the HTTP `101 Switching Protocols` response while preserving custom
/// PHP headers (such as `Set-Cookie`) emitted before `accept()`.
pub(crate) fn build_101_response(
    mut response: Response,
    accept_key: &str,
    subprotocol: Option<&str>,
    client_subprotocols: Option<&[u8]>,
) -> Response {
    response.status = 101;
    response.headers.retain(|(name, _)| {
        !name.eq_ignore_ascii_case("upgrade")
            && !name.eq_ignore_ascii_case("connection")
            && !name.eq_ignore_ascii_case("sec-websocket-accept")
            && !name.eq_ignore_ascii_case(SEC_WEBSOCKET_PROTOCOL)
            && !name.eq_ignore_ascii_case("content-length")
            && !name.eq_ignore_ascii_case("transfer-encoding")
    });
    response.body = crate::response::Body::default();
    response
        .headers
        .push(("upgrade".to_string(), b"websocket".to_vec()));
    response
        .headers
        .push(("connection".to_string(), b"Upgrade".to_vec()));
    response.headers.push((
        "sec-websocket-accept".to_string(),
        accept_key.as_bytes().to_vec(),
    ));
    if let Some(proto) = subprotocol
        && !proto.is_empty()
        && client_subprotocols
            .is_some_and(|offered| contains_token_ignore_ascii_case(offered, proto.as_bytes()))
    {
        response.headers.push((
            SEC_WEBSOCKET_PROTOCOL.to_string(),
            proto.as_bytes().to_vec(),
        ));
    }
    response
}

fn contains_token_ignore_ascii_case(header_value: &[u8], expected: &[u8]) -> bool {
    header_value
        .split(|&byte| byte == b',')
        .any(|part| part.trim_ascii().eq_ignore_ascii_case(expected))
}

fn is_valid_websocket_key(key: &[u8]) -> bool {
    if key.len() != 24 || &key[22..] != b"==" {
        return false;
    }
    key[..22]
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
}

fn split_origin(value: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = value.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if scheme.is_empty() || authority.is_empty() {
        return None;
    }
    Some((scheme, authority))
}

fn origins_equal(left: &str, right: &str) -> bool {
    let Some((ls, la)) = split_origin(left.trim()) else {
        return false;
    };
    let Some((rs, ra)) = split_origin(right.trim()) else {
        return false;
    };
    ls.eq_ignore_ascii_case(rs) && authorities_equal(la, ra, ls)
}

fn authorities_equal(left: &str, right: &str, scheme: &str) -> bool {
    let default_port = if scheme.eq_ignore_ascii_case("https") {
        "443"
    } else {
        "80"
    };
    let (lh, lp) = split_host_port(left, default_port);
    let (rh, rp) = split_host_port(right, default_port);
    lh.eq_ignore_ascii_case(rh) && lp == rp
}

fn split_host_port<'a>(authority: &'a str, default_port: &'a str) -> (&'a str, &'a str) {
    if let Some(bracketed_end) = authority.rfind(']')
        && authority.starts_with('[')
    {
        let host = &authority[..=bracketed_end];
        let port = authority[bracketed_end + 1..]
            .strip_prefix(':')
            .unwrap_or(default_port);
        return (host, port);
    }
    match authority.rsplit_once(':') {
        Some((host, port)) if !host.contains(':') && !port.is_empty() => (host, port),
        _ => (authority, default_port),
    }
}
