//! Per-connection HTTP/1.1 handling.

use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::header::{HeaderName, HeaderValue};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response as HyperResponse};
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::Watcher;
use tokio::net::TcpStream;

use crate::handler::Handler;
use crate::request::{HttpVersion, Request};
use crate::response::Response;

/// Serve one connection. Runs until the client or the server closes it; a
/// shutdown watcher finishes in-flight requests and stops new ones.
pub(super) async fn handle_connection(
    stream: TcpStream,
    peer: SocketAddr,
    local_addr: SocketAddr,
    handler: Rc<dyn Handler>,
    graceful: Watcher,
    max_body_bytes: usize,
    header_read_timeout: Duration,
) {
    let service = service_fn(move |hyper_request: HyperRequest<Incoming>| {
        let handler = handler.clone();
        async move {
            let request =
                match to_request(hyper_request, peer, local_addr, max_body_bytes).await {
                    Ok(request) => request,
                    Err(BodyError::TooLarge) => {
                        return Ok::<_, std::convert::Infallible>(to_hyper_response(
                            Response::text(413, "request body too large\n"),
                        ));
                    }
                    Err(BodyError::Read) => {
                        return Ok::<_, std::convert::Infallible>(to_hyper_response(
                            Response::text(400, "malformed request body\n"),
                        ));
                    }
                };
            // Run the handler on its own task so a panic becomes a 500
            // instead of tearing down the connection task or the worker.
            let task = tokio::task::spawn_local(async move { handler.handle(request).await });
            let response = match task.await {
                Ok(response) => response,
                Err(error) if error.is_panic() => Response::text(500, "internal server error\n"),
                Err(_) => Response::text(500, "internal server error\n"),
            };
            Ok(to_hyper_response(response))
        }
    });
    let connection = http1::Builder::new()
        .header_read_timeout(header_read_timeout)
        .timer(TokioTimer::new())
        .serve_connection(TokioIo::new(stream), service);
    if let Err(error) = graceful.watch(connection).await {
        eprintln!("phera-server: connection error: {error}");
    }
}

/// Turn a hyper request into a [`Request`] for the handler, buffering the
/// body under `max_body_bytes`.
async fn to_request(
    request: HyperRequest<Incoming>,
    peer: SocketAddr,
    local_addr: SocketAddr,
    max_body_bytes: usize,
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
    let body = read_body(body, max_body_bytes).await?;
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
fn to_hyper_response(response: Response) -> HyperResponse<Full<Bytes>> {
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
        .body(Full::new(Bytes::from(response.body)))
        .unwrap_or_else(|_| HyperResponse::new(Full::new(Bytes::new())))
}

enum BodyError {
    TooLarge,
    Read,
}

/// Read the whole body into memory. Once the limit is hit, remaining chunks
/// are drained (up to a small cap) rather than buffered, so the 413 the
/// client gets is clean and the connection stays reusable.
async fn read_body(mut body: Incoming, limit: usize) -> Result<Vec<u8>, BodyError> {
    const DRAIN_CAP: usize = 1024 * 1024;
    let mut buffer = Vec::new();
    let mut drained = 0usize;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|_| BodyError::Read)?;
        if let Some(data) = frame.data_ref() {
            if buffer.len() + data.len() <= limit {
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
