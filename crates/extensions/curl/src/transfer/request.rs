//! A redirect from `https://a.example` to another origin drops credentials by default.

use std::collections::HashSet;

use base64::{Engine, engine::general_purpose::STANDARD};
use bytes::Bytes;
use http_body_util::Full;
use hyper::{
    HeaderMap, Request, Version,
    header::{HeaderName, HeaderValue},
};
use url::Url;

use crate::{
    error::TransferError,
    options::{Auth, HttpVersion, Settings},
};

use super::upload::Payload;

pub(super) fn build(
    url: &Url,
    method: &str,
    payload: &Payload,
    settings: &Settings,
    privacy: Privacy<'_>,
) -> Result<(Request<Full<Bytes>>, i64), TransferError> {
    let mut clean = url.clone();
    let _ = clean.set_password(None);
    let _ = clean.set_username("");
    clean.set_fragment(None);
    let mut request = Request::builder()
        .method(method.as_bytes())
        .uri(clean.as_str())
        .version(match settings.transport.http {
            HttpVersion::OneZero => Version::HTTP_10,
            HttpVersion::TwoPrior => Version::HTTP_2,
            _ => Version::HTTP_11,
        })
        .body(Full::new(payload.bytes.clone()))
        .map_err(invalid)?;
    let headers = request.headers_mut();
    let mut overridden = HashSet::new();
    for line in &settings.headers {
        let (name, value) = parse(line)?;
        if !privacy.accept_credentials && matches!(name.as_str(), "authorization" | "cookie") {
            continue;
        }
        // These are hop-specific and must never be forwarded to an origin.
        if matches!(name.as_str(), "proxy-authorization" | "proxy-connection") {
            continue;
        }
        overridden.insert(name.as_str().to_owned());
        if let Some(value) = value {
            headers.append(name, value);
        }
    }
    let chunked = overridden.contains("transfer-encoding");
    let mut add = |name: &'static str, value: &[u8]| -> Result<(), TransferError> {
        if !overridden.contains(name) {
            headers.insert(
                HeaderName::from_static(name),
                HeaderValue::from_bytes(value).map_err(invalid)?,
            );
        }
        Ok(())
    };
    let authority = http_authority(&clean);
    add("host", authority.as_bytes())?;
    add("accept", b"*/*")?;
    if let Some(agent) = &settings.agent {
        add("user-agent", agent)?;
    }
    if let Some(referer) = privacy.referer {
        add("referer", referer)?;
    }
    if let Some(encoding) = &settings.encoding {
        add(
            "accept-encoding",
            if encoding.is_empty() {
                b"deflate, gzip, br"
            } else {
                encoding.as_bytes()
            },
        )?;
    }
    if payload.has_body {
        if let Some(media) = &payload.content_type {
            add("content-type", media.as_bytes())?;
        }
        if !chunked {
            add("content-length", payload.bytes.len().to_string().as_bytes())?;
        }
    }
    let mut cookies = privacy.cookies.to_owned().into_bytes();
    if privacy.accept_credentials
        && let Some(manual) = &settings.cookie
    {
        if !cookies.is_empty() {
            cookies.extend_from_slice(b"; ");
        }
        cookies.extend_from_slice(manual);
    }
    if !cookies.is_empty() {
        add("cookie", &cookies)?;
    }
    let mut auth = 0;
    if privacy.accept_credentials {
        if matches!(settings.auth, Auth::Bearer | Auth::BasicOrBearer)
            && let Some(bearer) = &settings.bearer
        {
            let mut value = b"Bearer ".to_vec();
            value.extend_from_slice(bearer);
            add("authorization", &value)?;
            auth = 64;
        } else if matches!(settings.auth, Auth::Basic | Auth::BasicOrBearer) {
            let userinfo = url_credentials(url);
            if let Some(username) = settings
                .username
                .as_deref()
                .or(userinfo.as_ref().map(|u| u.0.as_slice()))
            {
                let mut value = username.to_vec();
                value.push(b':');
                if let Some((_, password)) = &userinfo
                    && settings.username.is_none()
                {
                    value.extend_from_slice(password);
                } else {
                    value.extend_from_slice(&settings.password);
                }
                add(
                    "authorization",
                    format!("Basic {}", STANDARD.encode(value)).as_bytes(),
                )?;
                auth = 1;
            }
        }
    }
    if let Some(proxy) = privacy.proxy_auth {
        add("proxy-authorization", proxy)?;
    }
    if overridden.contains("authorization") {
        auth = 0;
    }
    Ok((request, auth))
}

pub(super) struct Privacy<'a> {
    pub(super) accept_credentials: bool,
    pub(super) cookies: &'a str,
    pub(super) referer: Option<&'a [u8]>,
    pub(super) proxy_auth: Option<&'a [u8]>,
}

fn url_credentials(url: &Url) -> Option<(Vec<u8>, Vec<u8>)> {
    let user = url.username();
    if user.is_empty() && url.password().is_none() {
        return None;
    }
    Some((
        percent_encoding::percent_decode_str(user).collect(),
        percent_encoding::percent_decode_str(url.password().unwrap_or_default()).collect(),
    ))
}

fn parse(line: &[u8]) -> Result<(HeaderName, Option<HeaderValue>), TransferError> {
    let (name, value) = if let Some(index) = line.iter().position(|&byte| byte == b':') {
        let value = line[index + 1..].trim_ascii();
        (&line[..index], (!value.is_empty()).then_some(value))
    } else if let Some(name) = line.strip_suffix(b";") {
        (name, Some(b"".as_slice()))
    } else {
        return Err(TransferError::new(43, "Invalid header"));
    };
    let name = HeaderName::from_bytes(name).map_err(invalid)?;
    let value = value
        .map(HeaderValue::from_bytes)
        .transpose()
        .map_err(invalid)?;
    Ok((name, value))
}

fn invalid(error: impl std::fmt::Display) -> TransferError {
    TransferError::new(43, format!("Invalid HTTP request: {error}"))
}

pub(super) fn http_authority(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_owned(),
    }
}

pub(super) fn capture(request: &Request<Full<Bytes>>) -> Vec<u8> {
    let target = request
        .uri()
        .path_and_query()
        .map_or("/", |target| target.as_str());
    let mut bytes =
        format!("{} {target} {:?}\r\n", request.method(), request.version()).into_bytes();
    for (name, value) in request.headers() {
        bytes.extend_from_slice(&line(name.as_str(), value.as_bytes()));
    }
    bytes.extend_from_slice(b"\r\n");
    bytes
}

pub(super) fn line(name: &str, value: &[u8]) -> Vec<u8> {
    let mut line = Vec::with_capacity(name.len() + value.len() + 4);
    line.extend_from_slice(name.as_bytes());
    line.extend_from_slice(b": ");
    line.extend_from_slice(value);
    line.extend_from_slice(b"\r\n");
    line
}

pub(super) fn auth_available(headers: &HeaderMap, header: &'static str) -> i64 {
    headers
        .get_all(header)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .fold(0, |all, value| {
            let scheme = value.split_ascii_whitespace().next().unwrap_or("");
            all | match scheme.to_ascii_lowercase().as_str() {
                "basic" => 1,
                "digest" => 2,
                "negotiate" => 4,
                "ntlm" => 8,
                "bearer" => 64,
                _ => 0,
            }
        })
}

pub(super) fn redirect_method(
    status: u16,
    method: crate::options::Method,
    keep: u8,
) -> crate::options::Method {
    use crate::options::Method::{Get, Head, Post};
    match (status, method) {
        (301, Post) if keep & 1 == 0 => Get,
        (302, Post) if keep & 2 == 0 => Get,
        (303, Post) if keep & 4 == 0 => Get,
        (303, other) if !matches!(other, Head | Post) => Get,
        _ => method,
    }
}
