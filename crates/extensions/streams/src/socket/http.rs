//! `fopen('http://localhost/results', 'r')` exposes HTTP response bytes and headers.

use std::{io, io::Cursor};

use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{Request, Uri, Version, header};
use hyper_util::rt::TokioIo;
use phera_extension_api::{ArrayKey, Context, NativeResult, PhpArray, Value, value::to_bool};
use tokio::io::{AsyncRead, AsyncWrite};

use super::{
    Io,
    address::{Address, Transport},
    default_timeout, tls, transport,
};
use crate::context_options;
use crate::stream::{Backing, Stream, Wrapper};

const MAX_BODY: usize = 16 * 1024 * 1024;

pub(crate) fn is_url(path: &[u8]) -> bool {
    path.get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"http://"))
        || path
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"https://"))
}

pub(crate) async fn fopen(
    context: &mut dyn Context,
    function: &str,
    name: &[u8],
    mode: &[u8],
    supplied: &Value,
) -> NativeResult {
    let path = String::from_utf8_lossy(name);
    if !crate::user_file::enabled(context, b"allow_url_fopen", true) {
        let scheme = if path.starts_with("https:") {
            "https"
        } else {
            "http"
        };
        context.warning(format!(
            "{function}(): {scheme}:// wrapper is disabled in the server configuration by allow_url_fopen=0"
        ));
        return failed(
            context,
            function,
            &path,
            "no suitable wrapper could be found",
        );
    }
    if !matches!(mode, b"r" | b"rb" | b"rt") {
        return failed(
            context,
            function,
            &path,
            "HTTP wrapper does not support writeable connections",
        );
    }
    let attached = if supplied.is_null() {
        context_options::default_for_wrapper(context)
    } else {
        supplied.clone()
    };
    let options = context_options::for_wrapper(&attached, "http");
    let ssl = context_options::for_wrapper(&attached, "ssl");
    let timeout = match option(&options, "timeout") {
        Some(Value::Float(seconds)) => std::time::Duration::try_from_secs_f64(*seconds).ok(),
        Some(Value::Int(seconds)) => u64::try_from(*seconds)
            .ok()
            .map(std::time::Duration::from_secs),
        _ => default_timeout(context),
    };
    let request = fetch(&path, &options, &ssl);
    let response = match timeout {
        Some(timeout) => tokio::time::timeout(timeout, request)
            .await
            .unwrap_or_else(|_| Err(io::ErrorKind::TimedOut.into())),
        None => request.await,
    };
    let (body, lines) = match response {
        Ok(response) => response,
        Err(error) => return failed(context, function, &path, &error.to_string()),
    };
    let mut stream = Stream::new(
        Backing::Memory(Cursor::new(body)),
        Wrapper::Http,
        mode,
        name,
    );
    stream.seekable = false;
    stream.data_header = Some(lines);
    Ok(crate::request::new_resource(
        context.state(),
        "stream",
        stream,
    ))
}

async fn fetch(path: &str, options: &PhpArray, ssl: &PhpArray) -> io::Result<(Vec<u8>, Vec<u8>)> {
    let uri: Uri = path.parse().map_err(invalid)?;
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
    if option(options, "proxy").is_some() {
        return Err(invalid("HTTP proxy context options are not supported"));
    }
    let request = build_request(uri.clone(), authority.as_str(), options)?;
    let socket = transport::connect(&address, None, false).await?;
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
        let secure = tls::take_channel(&socket)
            .await
            .ok_or_else(|| invalid("TLS setup failed"))?;
        secure
    } else {
        Box::new(socket.wire.clone())
    };
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(channel))
        .await
        .map_err(invalid)?;
    let task = ConnectionTask(tokio::spawn(connection));
    let response = sender.send_request(request).await.map_err(invalid)?;
    let (header, body) = response.into_parts();
    let version = if header.version == Version::HTTP_10 {
        "1.0"
    } else {
        "1.1"
    };
    let reason = header.status.canonical_reason().unwrap_or_default();
    let mut lines = format!("HTTP/{version} {} {reason}", header.status.as_u16()).into_bytes();
    if (header.status.is_client_error() || header.status.is_server_error())
        && !option(options, "ignore_errors").is_some_and(to_bool)
    {
        return Err(invalid(format!(
            "HTTP request failed! {}",
            String::from_utf8_lossy(&lines)
        )));
    }
    for (name, value) in &header.headers {
        lines.push(b'\n');
        lines.extend_from_slice(name.as_str().as_bytes());
        lines.extend_from_slice(b": ");
        lines.extend_from_slice(value.as_bytes());
    }
    let bytes = Limited::new(body, MAX_BODY)
        .collect()
        .await
        .map_err(invalid)?
        .to_bytes();
    drop(task);
    Ok((bytes.to_vec(), lines))
}

trait HttpChannel: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> HttpChannel for T {}

struct ConnectionTask(tokio::task::JoinHandle<hyper::Result<()>>);
impl Drop for ConnectionTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn build_request(uri: Uri, host: &str, options: &PhpArray) -> io::Result<Request<Full<Bytes>>> {
    let path = uri.path_and_query().map_or("/", |value| value.as_str());
    let method = match option(options, "method") {
        Some(Value::String(method)) => {
            hyper::Method::from_bytes(method.as_bytes()).map_err(invalid)?
        }
        _ => hyper::Method::GET,
    };
    let body = match option(options, "content") {
        Some(Value::String(body)) => Bytes::copy_from_slice(body.as_bytes()),
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
            Some(Value::String(value)) => {
                for line in value.as_bytes().split(|byte| *byte == b'\n') {
                    add_header(headers, line)?;
                }
            }
            Some(Value::Array(value)) => {
                for entry in value.values() {
                    if let Value::String(line) = &*entry.deref() {
                        add_header(headers, line.as_bytes())?;
                    }
                }
            }
            _ => (),
        }
        headers.insert(
            header::CONNECTION,
            header::HeaderValue::from_static("close"),
        );
    }
    builder.body(Full::new(body)).map_err(invalid)
}

fn add_header(headers: &mut header::HeaderMap, line: &[u8]) -> io::Result<()> {
    let Some(index) = line.iter().position(|&byte| byte == b':') else {
        return Ok(());
    };
    let name = header::HeaderName::from_bytes(&line[..index]).map_err(invalid)?;
    let value = header::HeaderValue::from_bytes(line[index + 1..].trim_ascii()).map_err(invalid)?;
    if name == header::HOST {
        headers.insert(name, value);
    } else {
        headers.append(name, value);
    }
    Ok(())
}

fn option<'a>(options: &'a PhpArray, key: &str) -> Option<&'a Value> {
    options.get(&ArrayKey::str(key))
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, error.to_string())
}

fn failed(context: &mut dyn Context, function: &str, path: &str, detail: &str) -> NativeResult {
    context.warning(format!(
        "{function}({path}): Failed to open stream: {detail}"
    ));
    Ok(Value::Bool(false))
}
