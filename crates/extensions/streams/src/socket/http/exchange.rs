//! Connects and sends the HTTP/1 request behind `fopen('https://example.com', 'r')`.

use std::io;

use bytes::Bytes;
use http_body_util::Full;
use hyper::{Request, Uri, Version, header};
use hyper_util::rt::TokioIo;
use phera_extension_api::{PhpArray, Value};
use tokio::io::{AsyncRead, AsyncWrite};

use super::{invalid, option};
use crate::socket::{
    Io,
    address::{Address, Transport},
    http_headers::{Capture, ReadHeaders},
    tls, transport,
};

pub(super) async fn exchange(
    uri: Uri,
    options: &PhpArray,
    ssl: &PhpArray,
    same_origin: bool,
    use_get: bool,
    authorization: Option<header::HeaderValue>,
) -> io::Result<(
    hyper::Response<hyper::body::Incoming>,
    Vec<Vec<u8>>,
    ConnectionTask,
)> {
    let authority = uri
        .authority()
        .ok_or_else(|| invalid("Invalid HTTP authority"))?;
    let encrypted = uri.scheme_str() == Some("https");
    let kind = if encrypted {
        Transport::Tls(121)
    } else {
        Transport::Tcp
    };
    let port = authority
        .port_u16()
        .unwrap_or(if encrypted { 443 } else { 80 });
    let target = format!("{}:{port}", authority.host());
    let address = Address::parse(kind, target.as_bytes()).map_err(invalid)?;
    let host = if authority.port_u16().is_some() {
        target.as_str()
    } else {
        authority.host()
    };
    let request = build_request(&uri, host, options, same_origin, use_get, authorization)?;
    let socket = transport::connect(&address, None, false, |_| {}).await?;
    let _ = socket.peer.set(address.host.to_owned());
    let channel: Box<dyn HttpChannel> = if encrypted {
        let connection = Io {
            socket: socket.clone(),
            blocked: true,
            timeout: None,
            chunk: 8192,
        };
        let options = tls::Options::parse(ssl, true)?;
        tls::enable(&connection, &options, options.method.unwrap_or(121) | 1).await?;
        tls::take_channel(&socket)
            .await
            .ok_or_else(|| invalid("TLS setup failed"))?
    } else {
        Box::new(socket.wire.clone())
    };
    let capture = Capture::default();
    let channel = ReadHeaders::new(channel, capture.clone());
    let (mut sender, connection) = hyper::client::conn::http1::Builder::new()
        .allow_obsolete_multiline_headers_in_responses(true)
        .handshake(TokioIo::new(channel))
        .await
        .map_err(invalid)?;
    let task = ConnectionTask(tokio::spawn(connection));
    let response = sender.send_request(request).await.map_err(invalid)?;
    Ok((response, capture.lines(), task))
}

trait HttpChannel: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> HttpChannel for T {}

pub(super) struct ConnectionTask(tokio::task::JoinHandle<hyper::Result<()>>);
impl Drop for ConnectionTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn build_request(
    uri: &Uri,
    host: &str,
    options: &PhpArray,
    same_origin: bool,
    use_get: bool,
    authorization: Option<header::HeaderValue>,
) -> io::Result<Request<Full<Bytes>>> {
    let path = uri.path_and_query().map_or("/", |value| value.as_str());
    let method = match option(options, "method").and_then(Value::as_str_bytes) {
        Some(method) if !use_get || method == b"HEAD" => {
            hyper::Method::from_bytes(method).map_err(invalid)?
        }
        _ => hyper::Method::GET,
    };
    let body = match option(options, "content").and_then(Value::as_str_bytes) {
        Some(body) if !use_get => Bytes::copy_from_slice(body),
        _ => Bytes::new(),
    };
    let version = match option(options, "protocol_version") {
        Some(Value::Float(number)) if *number == 1.0 => Version::HTTP_10,
        _ => Version::HTTP_11,
    };
    let mut builder = Request::builder().method(method).uri(path).version(version);
    if let Some(headers) = builder.headers_mut() {
        headers.insert(
            header::HOST,
            header::HeaderValue::from_str(host).map_err(invalid)?,
        );
        match option(options, "header") {
            Some(Value::InternedStr(value)) => {
                for line in value.as_bytes().split(|byte| *byte == b'\n') {
                    add_header(headers, line, same_origin, use_get)?;
                }
            }
            Some(Value::String(value)) => {
                for line in value.as_bytes().split(|byte| *byte == b'\n') {
                    add_header(headers, line, same_origin, use_get)?;
                }
            }
            Some(Value::Array(value)) => {
                for entry in value.values() {
                    if let Some(line) = entry.deref().as_str_bytes() {
                        add_header(headers, line, same_origin, use_get)?;
                    }
                }
            }
            _ => (),
        }
        if let Some(authorization) = authorization {
            headers
                .entry(header::AUTHORIZATION)
                .or_insert(authorization);
        }
        headers.insert(
            header::CONNECTION,
            header::HeaderValue::from_static("close"),
        );
    }
    builder.body(Full::new(body)).map_err(invalid)
}

fn add_header(
    headers: &mut header::HeaderMap,
    line: &[u8],
    same_origin: bool,
    use_get: bool,
) -> io::Result<()> {
    let Some(index) = line.iter().position(|&byte| byte == b':') else {
        return Ok(());
    };
    let name = header::HeaderName::from_bytes(&line[..index]).map_err(invalid)?;
    if !same_origin
        && matches!(
            name,
            header::HOST | header::AUTHORIZATION | header::PROXY_AUTHORIZATION | header::COOKIE
        )
        || use_get
            && matches!(
                name,
                header::CONTENT_LENGTH | header::CONTENT_TYPE | header::TRANSFER_ENCODING
            )
    {
        return Ok(());
    }
    let value = header::HeaderValue::from_bytes(line[index + 1..].trim_ascii()).map_err(invalid)?;
    if name == header::HOST {
        headers.insert(name, value);
    } else {
        headers.append(name, value);
    }
    Ok(())
}
