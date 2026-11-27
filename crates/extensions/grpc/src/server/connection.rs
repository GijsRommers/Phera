//! Sockets, HTTP/2 connections and streams, up to the point where a call
//! is ready for its PHP handler.
//!
//! Connections and streams are futures in one set that the driver polls
//! itself. A connection hands its new streams to the driver through a
//! queue, which keeps them in the same set; so `listen()` needs no
//! `LocalSet` and nothing here has to be `Send`.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use h2::RecvStream;
use h2::server::SendResponse;
use http::header::{CONTENT_TYPE, HeaderMap};
use http::{Method, Request, StatusCode};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;
use tokio_rustls::TlsAcceptor;

use super::response::{GRPC_ACCEPT_ENCODING, GRPC_ENCODING, Responder, http_error};
use super::{HEALTH_CHECK, Route, RouteKind};
use crate::framing::{self, Encoding};
use crate::headers;
use crate::protobuf::wire::{Reader, WireType};
use crate::status::{
    DEADLINE_EXCEEDED, INTERNAL, NOT_FOUND, OK, Status, UNAVAILABLE, UNIMPLEMENTED,
};
use crate::transport;

type Task = Pin<Box<dyn Future<Output = ()>>>;

/// What stream tasks need to know about the routes: no PHP values.
pub struct RouteTable {
    /// Sorted, for binary search; `kinds[i]` belongs to `paths[i]`.
    paths: Vec<Box<str>>,
    kinds: Vec<RouteKind>,
    /// The services that have a route, sorted, for health checks.
    services: Vec<Box<str>>,
    health: bool,
    max_message: usize,
    compress: bool,
}

impl RouteTable {
    /// `routes` must be sorted by path.
    pub fn new(routes: &[Route], health: bool, max_message: usize, compress: bool) -> Self {
        let mut services: Vec<Box<str>> = routes
            .iter()
            .filter_map(|route| {
                route.path[1..]
                    .split_once('/')
                    .map(|(service, _)| service.into())
            })
            .collect();
        services.sort();
        services.dedup();
        Self {
            paths: routes.iter().map(|route| route.path.clone()).collect(),
            kinds: routes.iter().map(|route| route.kind).collect(),
            services,
            health,
            max_message,
            compress,
        }
    }

    fn find(&self, path: &str) -> Option<usize> {
        self.paths
            .binary_search_by(|probe| (**probe).cmp(path))
            .ok()
    }
}

/// A call whose request has arrived in full, for the PHP worker.
pub struct Call {
    /// Index into the sorted routes.
    pub route: usize,
    pub messages: Vec<Bytes>,
    pub headers: HeaderMap,
    pub deadline: Option<Instant>,
    pub peer: SocketAddr,
    pub responder: Responder,
}

pub struct Driver {
    pub listener: TcpListener,
    pub tls: Option<TlsAcceptor>,
    pub table: Rc<RouteTable>,
    pub calls: mpsc::UnboundedSender<Call>,
    pub shutdown: Rc<watch::Sender<bool>>,
    pub grace: Duration,
    pub handle_signals: bool,
}

/// Accepts connections until shutdown, then lets them finish their calls
/// for at most the grace period.
pub async fn drive(driver: Driver) {
    let Driver {
        listener,
        tls,
        table,
        calls,
        shutdown,
        grace,
        handle_signals,
    } = driver;
    let (spawn, mut spawned) = mpsc::unbounded_channel::<Task>();
    let mut tasks: FuturesUnordered<Task> = FuturesUnordered::new();
    let mut stopping = shutdown.subscribe();
    let mut signals = Signals::new(handle_signals);
    loop {
        tokio::select! {
            biased;
            Some(task) = spawned.recv() => tasks.push(task),
            Some(()) = tasks.next(), if !tasks.is_empty() => {}
            _ = stopping.changed() => break,
            () = signals.recv() => {
                shutdown.send_replace(true);
                break;
            }
            accepted = listener.accept() => match accepted {
                Ok((socket, peer)) => {
                    let _ = socket.set_nodelay(true);
                    tasks.push(Box::pin(connection(
                        socket,
                        peer,
                        tls.clone(),
                        table.clone(),
                        calls.clone(),
                        spawn.clone(),
                        shutdown.subscribe(),
                    )));
                }
                // Out of descriptors and the like: try again shortly.
                Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
            },
        }
    }
    drop(listener);
    drop(calls);
    drop(spawn);
    let drain = async {
        loop {
            while let Ok(task) = spawned.try_recv() {
                tasks.push(task);
            }
            if tasks.is_empty() {
                break;
            }
            tokio::select! {
                biased;
                Some(task) = spawned.recv() => tasks.push(task),
                _ = tasks.next() => {}
            }
        }
    };
    let _ = tokio::time::timeout(grace, drain).await;
}

/// SIGINT and SIGTERM start a graceful shutdown, unless the script asked
/// to keep the default signal behaviour (`'handleSignals' => false`).
struct Signals {
    #[cfg(unix)]
    streams: Option<(tokio::signal::unix::Signal, tokio::signal::unix::Signal)>,
}

impl Signals {
    fn new(enabled: bool) -> Self {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            let streams = enabled
                .then(|| {
                    Some((
                        signal(SignalKind::interrupt()).ok()?,
                        signal(SignalKind::terminate()).ok()?,
                    ))
                })
                .flatten();
            Self { streams }
        }
        #[cfg(not(unix))]
        {
            let _ = enabled;
            Self {}
        }
    }

    async fn recv(&mut self) {
        #[cfg(unix)]
        if let Some((interrupt, terminate)) = &mut self.streams {
            tokio::select! {
                _ = interrupt.recv() => return,
                _ = terminate.recv() => return,
            }
        }
        std::future::pending::<()>().await
    }
}

async fn connection(
    socket: TcpStream,
    peer: SocketAddr,
    tls: Option<TlsAcceptor>,
    table: Rc<RouteTable>,
    calls: mpsc::UnboundedSender<Call>,
    spawn: mpsc::UnboundedSender<Task>,
    shutdown: watch::Receiver<bool>,
) {
    const HANDSHAKE: Duration = Duration::from_secs(10);
    match tls {
        Some(acceptor) => {
            if let Ok(Ok(stream)) = tokio::time::timeout(HANDSHAKE, acceptor.accept(socket)).await {
                http2(stream, peer, table, calls, spawn, shutdown).await;
            }
        }
        None => http2(socket, peer, table, calls, spawn, shutdown).await,
    }
}

async fn http2<T: AsyncRead + AsyncWrite + Unpin + 'static>(
    io: T,
    peer: SocketAddr,
    table: Rc<RouteTable>,
    calls: mpsc::UnboundedSender<Call>,
    spawn: mpsc::UnboundedSender<Task>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut builder = h2::server::Builder::new();
    builder
        .initial_window_size(1 << 20)
        .initial_connection_window_size(1 << 21)
        .max_concurrent_streams(1024);
    let handshake = builder.handshake::<T, Bytes>(io);
    let Ok(Ok(mut connection)) = tokio::time::timeout(Duration::from_secs(10), handshake).await
    else {
        return;
    };
    let mut draining = *shutdown.borrow();
    if draining {
        connection.graceful_shutdown();
    }
    loop {
        tokio::select! {
            accepted = connection.accept() => match accepted {
                Some(Ok((request, respond))) => {
                    let task = stream(request, respond, peer, table.clone(), calls.clone());
                    let _ = spawn.send(Box::pin(task));
                }
                Some(Err(_)) | None => break,
            },
            _ = shutdown.changed(), if !draining => {
                draining = true;
                connection.graceful_shutdown();
            }
        }
    }
}

/// Checks one request and reads it; answers it here when no PHP handler is
/// involved, queues it for the worker otherwise.
async fn stream(
    request: Request<RecvStream>,
    respond: SendResponse<Bytes>,
    peer: SocketAddr,
    table: Rc<RouteTable>,
    calls: mpsc::UnboundedSender<Call>,
) {
    let (parts, mut body) = request.into_parts();
    if parts.method != Method::POST {
        return http_error(respond, StatusCode::METHOD_NOT_ALLOWED);
    }
    let grpc = parts
        .headers
        .get(CONTENT_TYPE)
        .is_some_and(|value| value.as_bytes().starts_with(b"application/grpc"));
    if !grpc {
        return http_error(respond, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }
    let requested = parts
        .headers
        .get(GRPC_ENCODING)
        .map(|value| value.as_bytes());
    let Some(encoding) = Encoding::parse(requested) else {
        let status = framing::unsupported_encoding(requested.unwrap_or_default());
        return fail(Responder::new(respond, Encoding::Identity), status);
    };
    let accepts_gzip = Encoding::accepts_gzip(
        parts
            .headers
            .get(GRPC_ACCEPT_ENCODING)
            .map(|v| v.as_bytes()),
    );
    // Answer in kind: a gzip request gets a gzip response, as does every
    // client that accepts it when the server option asks for compression.
    let response_encoding = if accepts_gzip && (encoding == Encoding::Gzip || table.compress) {
        Encoding::Gzip
    } else {
        Encoding::Identity
    };
    let responder = Responder::new(respond, response_encoding);
    let deadline = parts
        .headers
        .get("grpc-timeout")
        .and_then(|value| headers::parse_timeout(value.as_bytes()))
        .map(|timeout| Instant::now() + timeout);
    let path = parts.uri.path();
    let route = table.find(path);
    if route.is_none() && !(table.health && path == HEALTH_CHECK) {
        let status = Status::new(UNIMPLEMENTED, format!("Method not found: {path}"));
        return fail(responder, status);
    }

    let read = transport::read_frames(&mut body, table.max_message);
    let frames = match deadline {
        Some(deadline) => match tokio::time::timeout_at(deadline, read).await {
            Ok(frames) => frames,
            Err(_) => {
                return fail(
                    responder,
                    Status::new(DEADLINE_EXCEEDED, "Deadline Exceeded"),
                );
            }
        },
        None => read.await,
    };
    let messages: Result<Vec<Bytes>, Status> = frames.and_then(|frames| {
        frames
            .into_iter()
            .map(|frame| frame.into_message(encoding, table.max_message))
            .collect()
    });
    let messages = match messages {
        Ok(messages) => messages,
        Err(status) => return fail(responder, status),
    };
    let Some(route) = route else {
        return health(responder, &table, &messages).await;
    };
    if table.kinds[route] != RouteKind::ClientStream && messages.len() != 1 {
        let status = Status::new(
            INTERNAL,
            format!("expected one request message, got {}", messages.len()),
        );
        return fail(responder, status);
    }
    let call = Call {
        route,
        messages,
        headers: parts.headers,
        deadline,
        peer,
        responder,
    };
    if let Err(mpsc::error::SendError(call)) = calls.send(call) {
        fail(
            call.responder,
            Status::new(UNAVAILABLE, "the server is shutting down"),
        );
    }
}

fn fail(responder: Responder, status: Status) {
    let _ = responder.finish(&status, HeaderMap::new(), HeaderMap::new());
}

/// `grpc.health.v1.Health/Check`: SERVING for the whole server (an empty
/// service name) and for every service with a route; NOT_FOUND otherwise.
async fn health(mut responder: Responder, table: &RouteTable, messages: &[Bytes]) {
    let [request] = messages else {
        let status = Status::new(INTERNAL, "expected one request message");
        return fail(responder, status);
    };
    // HealthCheckRequest { string service = 1; }
    let mut service: &[u8] = b"";
    let mut reader = Reader::new(request);
    while !reader.is_empty() {
        let field = match reader.tag() {
            Ok(field) => field,
            Err(error) => return fail(responder, Status::new(INTERNAL, error.to_string())),
        };
        let read = match field {
            (1, WireType::Len) => reader.len_delimited().map(|bytes| service = bytes),
            (_, wire) => reader.skip(wire),
        };
        if let Err(error) = read {
            return fail(responder, Status::new(INTERNAL, error.to_string()));
        }
    }
    let service = String::from_utf8_lossy(service);
    let known = service.is_empty()
        || service == "grpc.health.v1.Health"
        || table
            .services
            .binary_search_by(|probe| (**probe).cmp(&service))
            .is_ok();
    if !known {
        return fail(
            responder,
            Status::new(NOT_FOUND, format!("unknown service {service}")),
        );
    }
    // HealthCheckResponse { ServingStatus status = 1; }, SERVING = 1.
    let frame = framing::frame(&[0x08, 0x01], responder.encoding);
    if responder.send(frame).await.is_ok() {
        let _ = responder.finish(&Status::new(OK, ""), HeaderMap::new(), HeaderMap::new());
    }
}
