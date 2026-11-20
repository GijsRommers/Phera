//! Per-connection HTTP handling. A PHP `flush()` becomes an HTTP/1.1 chunk
//! or HTTP/2 DATA.

use std::future::Future;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;

use http_body_util::BodyExt;
use hyper::body::{Body as HyperBody, Bytes, Incoming};
use hyper::header::{HeaderName, HeaderValue};
use hyper::server::conn::{http1, http2};
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response as HyperResponse};
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulConnection;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::watch;
use tokio::time::Instant;

use crate::config::ConnectionLimits;
use crate::file_body::WireBody;
use crate::handler::Handler;
use crate::protocol::{self, Protocol};
use crate::receive_timeout::{HeadTimer, TimeoutStream};
use crate::request::{HttpVersion, Request};
use crate::response::Response;
use crate::response_task::{ConnectionWork, answer};

/// Serve one connection. Runs until the client or the server closes it; a
/// shutdown watcher finishes in-flight requests and stops new ones.
pub(super) async fn handle_connection(
    mut stream: TcpStream,
    peer: SocketAddr,
    local_addr: SocketAddr,
    handler: Rc<dyn Handler>,
    mut shutdown: watch::Receiver<()>,
    limits: ConnectionLimits,
    work: Rc<ConnectionWork>,
) {
    let accepted = Instant::now();
    let read_limit = limits.idle_timeout.min(limits.request_read_timeout);
    let detected = tokio::select! {
        biased;
        _ = shutdown.changed() => return,
        result = tokio::time::timeout_at(accepted + read_limit, protocol::detect(&mut stream)) => result,
    };
    let (protocol, read_ahead) = match detected {
        Ok(Ok(protocol)) => protocol,
        Ok(Err(_)) => return,
        Err(_) => {
            if limits.request_read_timeout <= limits.idle_timeout {
                reply_to_head_timeout(&mut stream).await;
            }
            return;
        }
    };
    let timer = HeadTimer::new(accepted);
    let head_timer = timer.clone();
    let service = service_fn(move |hyper_request: HyperRequest<Incoming>| {
        let handler = handler.clone();
        let work = work.clone();
        let started = (!hyper_request.body().is_end_stream()).then(|| head_timer.started());
        async move {
            let http2 = hyper_request.version() == hyper::Version::HTTP_2;
            let response = match to_request(hyper_request, peer, local_addr, limits, started).await
            {
                Ok(request) => answer(handler, request, limits.request_timeout, work).await,
                Err(BodyError::TooLarge) => Response::text(413, "request body too large\n"),
                Err(BodyError::Read) => Response::text(400, "malformed request body\n"),
                // Part of the body is still on its way, or never comes:
                // the connection cannot carry another request.
                Err(BodyError::Stalled) => {
                    let response = Response::text(408, "request read timeout\n");
                    if http2 {
                        response
                    } else {
                        response.with_header("connection", "close")
                    }
                }
            };
            Ok::<_, std::convert::Infallible>(to_hyper_response(response))
        }
    });
    let mut stream = TimeoutStream::new(stream, read_ahead);
    let io = TokioIo::new(stream.clone());
    let result = match protocol {
        Protocol::Http1 => {
            let connection = http1::Builder::new()
                .header_read_timeout(read_limit)
                .timer(timer)
                .serve_connection(io, service);
            let result = until_shutdown(connection, shutdown).await;
            if result.as_ref().is_err_and(hyper::Error::is_timeout)
                && limits.request_read_timeout <= limits.idle_timeout
            {
                reply_to_head_timeout(&mut stream).await;
            }
            result
        }
        Protocol::Http2 => {
            let connection = http2::Builder::new(LocalExecutor)
                .timer(TokioTimer::new())
                .keep_alive_interval(Some(limits.idle_timeout))
                .serve_connection(io, service);
            until_shutdown(connection, shutdown).await
        }
    };
    if let Err(error) = result
        && !error.is_timeout()
        && !error.is_incomplete_message()
    {
        eprintln!("phera-server: connection error: {error}");
    }
}

async fn reply_to_head_timeout(stream: &mut (impl tokio::io::AsyncWrite + Unpin)) {
    const TIMEOUT: &[u8] =
        b"HTTP/1.1 408 Request Timeout\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
    let _ = tokio::time::timeout(Duration::from_secs(1), stream.write_all(TIMEOUT)).await;
}

async fn until_shutdown<C: GracefulConnection>(
    connection: C,
    mut shutdown: watch::Receiver<()>,
) -> Result<(), C::Error> {
    let mut connection = std::pin::pin!(connection);
    tokio::select! {
        biased;
        _ = shutdown.changed() => {
            connection.as_mut().graceful_shutdown();
            connection.await
        }
        result = connection.as_mut() => result,
    }
}

#[derive(Clone, Copy)]
struct LocalExecutor;

impl<F: Future<Output = ()> + 'static> hyper::rt::Executor<F> for LocalExecutor {
    fn execute(&self, future: F) {
        tokio::task::spawn_local(future);
    }
}

/// Turn a hyper request into a [`Request`] for the handler, buffering the
/// body under `max_body_bytes`.
async fn to_request(
    request: HyperRequest<Incoming>,
    peer: SocketAddr,
    local_addr: SocketAddr,
    limits: ConnectionLimits,
    started: Option<Instant>,
) -> Result<Request, BodyError> {
    let (parts, body) = request.into_parts();
    let version = match parts.version {
        hyper::Version::HTTP_10 => HttpVersion::Http10,
        hyper::Version::HTTP_2 => HttpVersion::Http2,
        _ => HttpVersion::Http11,
    };
    let path = parts.uri.path().to_string();
    let query = parts.uri.query().unwrap_or_default().to_string();
    // The raw target as the client sent it; hyper does not decode it.
    let target = parts
        .uri
        .path_and_query()
        .map_or_else(|| path.clone(), |target| target.as_str().to_string());
    let mut headers: Vec<_> = parts
        .headers
        .iter()
        .map(|(name, value)| (name.as_str().to_string(), value.as_bytes().to_vec()))
        .collect();
    if version == HttpVersion::Http2
        && let Some(authority) = parts.uri.authority()
        && !parts.headers.contains_key(hyper::header::HOST)
    {
        headers.push(("host".into(), authority.as_str().as_bytes().to_vec()));
    }
    let body = match started {
        Some(started) => read_body(body, limits, started).await?,
        None => Vec::new(),
    };
    Ok(Request {
        method: parts.method.as_str().to_string(),
        target,
        path,
        query,
        version,
        headers,
        body,
        remote_addr: peer,
        local_addr,
    })
}

/// Turn a handler's [`Response`] into a hyper response.
fn to_hyper_response(response: Response) -> HyperResponse<WireBody> {
    let mut wire = HyperResponse::new(WireBody::from(response.body));
    *wire.status_mut() = hyper::StatusCode::from_u16(response.status)
        .unwrap_or(hyper::StatusCode::INTERNAL_SERVER_ERROR);
    for (name, value) in response.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::try_from(name.as_str()),
            HeaderValue::from_maybe_shared(Bytes::from(value)),
        ) {
            // The server alone says where a response ends. A handler (a PHP
            // script calling `header('Content-Length: 5')`) could otherwise
            // announce fewer bytes than it sends, and whatever reads the
            // connection next would take the rest for a second response.
            // hyper sets the right length from the body below.
            if name == hyper::header::CONTENT_LENGTH || name == hyper::header::TRANSFER_ENCODING {
                continue;
            }
            wire.headers_mut().append(name, value);
        }
    }
    wire
}

enum BodyError {
    TooLarge,
    Read,
    /// The client stopped sending in the middle of the body.
    Stalled,
}

/// Read the whole body into memory. Once the limit is hit, remaining chunks
/// are drained (up to a small cap) rather than buffered, so the 413 the
/// client gets is clean and the connection stays reusable.
async fn read_body(
    mut body: Incoming,
    limits: ConnectionLimits,
    started: Instant,
) -> Result<Vec<u8>, BodyError> {
    const DRAIN_CAP: usize = 1024 * 1024;
    let mut buffer = Vec::new();
    let mut drained = 0usize;
    loop {
        let budget = limits
            .request_read_timeout
            .saturating_add(super::receive_timeout::body_time_credit(buffer.len()));
        let remaining = budget.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            return Err(BodyError::Stalled);
        }
        let next =
            tokio::time::timeout(remaining.min(limits.body_idle_timeout), body.frame()).await;
        let frame = match next {
            Err(_silent_too_long) => return Err(BodyError::Stalled),
            Ok(None) => break,
            Ok(Some(frame)) => frame.map_err(|_| BodyError::Read)?,
        };
        if let Some(data) = frame.data_ref() {
            if buffer.len() + data.len() <= limits.max_body_bytes {
                buffer.extend_from_slice(data);
            } else {
                drained += data.len();
                if drained > DRAIN_CAP {
                    return Err(BodyError::TooLarge);
                }
            }
        }
    }
    if drained > 0 {
        Err(BodyError::TooLarge)
    } else {
        Ok(buffer)
    }
}
