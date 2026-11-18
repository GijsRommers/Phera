//! Per-connection HTTP/1.1 handling.

use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;

use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::header::{HeaderName, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response as HyperResponse};
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::Watcher;
use tokio::net::TcpStream;

use crate::config::ConnectionLimits;
use crate::file_body::WireBody;
use crate::handler::Handler;
use crate::request::{HttpVersion, Request};
use crate::response::{Body, Response};

/// Serve one connection. Runs until the client or the server closes it; a
/// shutdown watcher finishes in-flight requests and stops new ones.
pub(super) async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    local_addr: SocketAddr,
    handler: Rc<dyn Handler>,
    graceful: Watcher,
    limits: ConnectionLimits,
) {
    let service = service_fn(move |hyper_request: HyperRequest<Incoming>| {
        let handler = handler.clone();
        async move {
            let response = match to_request(hyper_request, peer, local_addr, limits).await {
                Ok(request) => answer(handler, request, limits.request_timeout).await,
                Err(BodyError::TooLarge) => Response::text(413, "request body too large\n"),
                Err(BodyError::Read) => Response::text(400, "malformed request body\n"),
                // Part of the body is still on its way, or never comes:
                // the connection cannot carry another request.
                Err(BodyError::Stalled) => Response::text(408, "the request body stopped\n")
                    .with_header("connection", "close"),
            };
            Ok::<_, std::convert::Infallible>(to_hyper_response(response))
        }
    });
    let connection = http1::Builder::new()
        // hyper's one timer for "waiting for a request head" covers a new
        // connection, a kept-alive one between requests, and a slow head.
        .header_read_timeout(limits.idle_timeout)
        .timer(TokioTimer::new())
        .serve_connection(TokioIo::new(stream), service);
    if let Err(error) = graceful.watch(connection).await
        && !is_ordinary_end(&error)
    {
        eprintln!("phera-server: connection error: {error}");
    }
}

/// A connection that sat idle until its time was up, or whose client left
/// in the middle of a request, is everyday traffic and not worth a log line.
fn is_ordinary_end(error: &hyper::Error) -> bool {
    error.is_timeout() || error.is_incomplete_message()
}

/// Runs the handler for one request, for at most `time_limit`.
async fn answer(
    handler: Rc<dyn Handler>,
    request: Request,
    time_limit: Option<Duration>,
) -> Response {
    // The handler runs on its own task so that a panic becomes a 500
    // instead of tearing down the connection task or the worker.
    let mut task = tokio::task::spawn_local(async move { handler.handle(request).await });
    let finished = match time_limit {
        None => (&mut task).await,
        Some(limit) => match tokio::time::timeout(limit, &mut task).await {
            Ok(finished) => finished,
            Err(_too_long) => {
                // Stopping the task drops everything the request held. It
                // takes effect the next time the handler waits or yields,
                // which a running PHP script does regularly.
                task.abort();
                return Response::text(504, "the request took too long\n");
            }
        },
    };
    finished.unwrap_or_else(|_| Response::text(500, "internal server error\n"))
}

/// Turn a hyper request into a [`Request`] for the handler, buffering the
/// body under `max_body_bytes`.
async fn to_request(
    request: HyperRequest<Incoming>,
    peer: SocketAddr,
    local_addr: SocketAddr,
    limits: ConnectionLimits,
) -> Result<Request, BodyError> {
    let (parts, body) = request.into_parts();
    let version = match parts.version {
        hyper::Version::HTTP_10 => HttpVersion::Http10,
        _ => HttpVersion::Http11,
    };
    let path = parts.uri.path().to_string();
    let query = parts.uri.query().unwrap_or_default().to_string();
    // The raw target as the client sent it; hyper does not decode it.
    let target = parts
        .uri
        .path_and_query()
        .map_or_else(|| path.clone(), |target| target.as_str().to_string());
    let headers = parts
        .headers
        .iter()
        .map(|(name, value)| (name.as_str().to_string(), value.as_bytes().to_vec()))
        .collect();
    let body = read_body(body, limits).await?;
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
    let status = hyper::StatusCode::from_u16(response.status)
        .unwrap_or(hyper::StatusCode::INTERNAL_SERVER_ERROR);
    let mut builder = HyperResponse::builder().status(status);
    for (name, value) in &response.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::try_from(name.as_str()),
            HeaderValue::from_bytes(value),
        ) {
            // The server alone says where a response ends. A handler (a PHP
            // script calling `header('Content-Length: 5')`) could otherwise
            // announce fewer bytes than it sends, and whatever reads the
            // connection next would take the rest for a second response.
            // hyper sets the right length from the body below.
            if name == hyper::header::CONTENT_LENGTH || name == hyper::header::TRANSFER_ENCODING {
                continue;
            }
            builder = builder.header(name, value);
        }
    }
    builder
        .body(WireBody::from(response.body))
        .unwrap_or_else(|_| HyperResponse::new(WireBody::from(Body::default())))
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
async fn read_body(mut body: Incoming, limits: ConnectionLimits) -> Result<Vec<u8>, BodyError> {
    const DRAIN_CAP: usize = 1024 * 1024;
    let mut buffer = Vec::new();
    let mut drained = 0usize;
    loop {
        // The clock starts again with every piece: a slow upload is fine, a
        // silent one is not.
        let next = tokio::time::timeout(limits.body_idle_timeout, body.frame()).await;
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
