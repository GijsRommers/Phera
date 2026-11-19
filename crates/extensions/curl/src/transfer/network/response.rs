//! `curl_getinfo($handle)` is updated before invoking response header callbacks.

use std::time::Instant;

use hyper::{Response, Version, body::Incoming};
use url::Url;

use crate::{connector::ConnectionInfo, info::Info};

use super::request;

pub(super) fn info(info: &mut Info, response: &Response<Incoming>, url: &Url, started: Instant) {
    let duration = |instant: Instant| instant.saturating_duration_since(started);
    info.status = i64::from(response.status().as_u16());
    info.scheme = Some(url.scheme().to_owned());
    info.http_version = match response.version() {
        Version::HTTP_10 => 1,
        Version::HTTP_11 => 2,
        Version::HTTP_2 => 3,
        _ => 0,
    };
    info.content_type = response
        .headers()
        .get("content-type")
        .map(|value| value.as_bytes().to_vec());
    info.download_length = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok()?.parse::<i64>().ok());
    info.auth_available = request::auth_available(response.headers(), "www-authenticate");
    info.proxy_auth_available = request::auth_available(response.headers(), "proxy-authenticate");
    info.retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|value| {
            let value = value.to_str().ok()?;
            value.parse().ok().or_else(|| {
                let date = httpdate::parse_http_date(value).ok()?;
                let future = date.duration_since(std::time::SystemTime::now()).ok()?;
                Some(future.as_secs() as i64)
            })
        })
        .unwrap_or(0);
    info.timing.first_byte = started.elapsed();
    if let Some(connection) = response.extensions().get::<ConnectionInfo>() {
        info.peer = connection.peer;
        info.local = connection.local;
        if connection.connected >= started && info.connection_id != Some(connection.id) {
            info.new_connections += 1;
            info.timing.lookup = duration(connection.lookup);
            info.timing.connect = duration(connection.connected);
            info.timing.tls = connection.tls.map_or(std::time::Duration::ZERO, duration);
        }
        info.timing.pretransfer = duration(connection.tls.unwrap_or(connection.connected));
        info.connection_id = Some(connection.id);
        info.proxy = connection.proxy;
        info.proxy_connect_code = connection.proxy_connect_code;
        info.proxy_auth = i64::from(connection.proxy_auth);
    }
}

pub(super) fn headers(response: &Response<Incoming>) -> Vec<Vec<u8>> {
    let version = match response.version() {
        Version::HTTP_10 => "HTTP/1.0",
        Version::HTTP_2 => "HTTP/2",
        _ => "HTTP/1.1",
    };
    let status = response.status();
    let reason = if response.version() == Version::HTTP_2 {
        ""
    } else {
        status.canonical_reason().unwrap_or("")
    };
    let mut headers = vec![format!("{version} {} {reason}\r\n", status.as_u16()).into_bytes()];
    for (name, value) in response.headers() {
        headers.push(request::line(name.as_str(), value.as_bytes()));
    }
    headers.push(b"\r\n".to_vec());
    headers
}
