//! Zero-idle-buffer HTTP/1.1 fast path powered by `httparse`.
//!
//! Idle keep-alive connections retain no read or write buffers and keep their
//! async task future compact (< 2 KiB): cold fallbacks (`handle_hyper_connection`,
//! streaming/file responses) are boxed off the hot future so they never bloat
//! idle keep-alive sockets. Each connection sleeps directly on
//! `TcpStream::readable()` and borrows a reusable 16 KiB buffer from its
//! worker's [`WorkerScratch`] only while parsing a request or flushing a
//! response.

use std::cell::{Cell, RefCell};
use std::future::{Future, poll_fn};
use std::io::{self, IoSlice};
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::fs::File;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::config::ConnectionLimits;
use crate::connection::{
    ConnectionParams, WebSocketWorkerContext, handle_hyper_connection, wait_for_shutdown,
};
use crate::handler::Handler;
use crate::receive_timeout::{H2_PREFACE, body_time_credit};
use crate::request::{HttpVersion, Request};
use crate::response::{Body, Response};
use crate::response_task::{ConnectionWork, DetachedRequests, answer};
use crate::websocket::handshake::UpgradeRejection;

const SCRATCH_CAP: usize = 16 * 1024;
const MAX_POOLED_BUFS: usize = 8;
const MAX_FAST_HEAD_BYTES: usize = 16 * 1024;
const MAX_FAST_BODY_BYTES: usize = 64 * 1024;
const MAX_FAST_HEADERS: usize = 64;
const FILE_CHUNK_BYTES: usize = 64 * 1024;

/// Per-worker reusable scratch buffers and 1-second cached HTTP `Date` header.
///
/// Kept on the worker thread (`!Send`), so borrowing and returning a buffer is
/// a single non-atomic `Vec::pop` / `Vec::push`.
pub(crate) struct WorkerScratch {
    read_bufs: RefCell<Vec<Vec<u8>>>,
    write_bufs: RefCell<Vec<Vec<u8>>>,
    date_cache: RefCell<(u64, [u8; 29])>,
}

impl WorkerScratch {
    pub(crate) fn new() -> Self {
        Self {
            read_bufs: RefCell::new(Vec::with_capacity(MAX_POOLED_BUFS)),
            write_bufs: RefCell::new(Vec::with_capacity(MAX_POOLED_BUFS)),
            date_cache: RefCell::new((0, [b' '; 29])),
        }
    }

    fn take_read_buf(&self) -> Vec<u8> {
        match self.read_bufs.borrow_mut().pop() {
            Some(mut buf) => {
                buf.clear();
                buf
            }
            None => Vec::with_capacity(SCRATCH_CAP),
        }
    }

    fn return_read_buf(&self, mut buf: Vec<u8>) {
        if buf.capacity() > MAX_FAST_BODY_BYTES + MAX_FAST_HEAD_BYTES {
            return;
        }
        buf.clear();
        let mut pool = self.read_bufs.borrow_mut();
        if pool.len() < MAX_POOLED_BUFS {
            pool.push(buf);
        }
    }

    fn take_write_buf(&self) -> Vec<u8> {
        match self.write_bufs.borrow_mut().pop() {
            Some(mut buf) => {
                buf.clear();
                buf
            }
            None => Vec::with_capacity(SCRATCH_CAP),
        }
    }

    fn return_write_buf(&self, mut buf: Vec<u8>) {
        if buf.capacity() > 128 * 1024 {
            return;
        }
        buf.clear();
        let mut pool = self.write_bufs.borrow_mut();
        if pool.len() < MAX_POOLED_BUFS {
            pool.push(buf);
        }
    }

    fn write_date_header(&self, out: &mut Vec<u8>) {
        let now = SystemTime::now();
        let secs = now
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs());
        let mut cache = self.date_cache.borrow_mut();
        if cache.0 != secs {
            let formatted = httpdate::fmt_http_date(now);
            let bytes = formatted.as_bytes();
            if bytes.len() == 29 {
                cache.1.copy_from_slice(bytes);
                cache.0 = secs;
            } else {
                out.extend_from_slice(b"date: ");
                out.extend_from_slice(bytes);
                out.extend_from_slice(b"\r\n");
                return;
            }
        }
        out.extend_from_slice(b"date: ");
        out.extend_from_slice(&cache.1);
        out.extend_from_slice(b"\r\n");
    }
}

/// Shared worker-local state referenced by every connection via a single `Rc`,
/// keeping per-connection idle task memory minimal.
pub(crate) struct WorkerContext {
    pub(crate) local_addr: SocketAddr,
    pub(crate) handler: Rc<dyn Handler>,
    pub(crate) limits: ConnectionLimits,
    pub(crate) ws: WebSocketWorkerContext,
    pub(crate) detached: Rc<DetachedRequests>,
    pub(crate) active_connections: Cell<usize>,
    pub(crate) scratch: WorkerScratch,
    /// Held alive until every connection on this worker finishes, waking
    /// graceful shutdown drain automatically when dropped.
    pub(crate) _http_done: mpsc::Sender<()>,
}

struct ActiveConnectionGuard {
    worker: Rc<WorkerContext>,
}

impl ActiveConnectionGuard {
    fn enter(worker: Rc<WorkerContext>) -> Self {
        worker
            .active_connections
            .set(worker.active_connections.get() + 1);
        Self { worker }
    }
}

impl Drop for ActiveConnectionGuard {
    fn drop(&mut self) {
        self.worker
            .active_connections
            .set(self.worker.active_connections.get().saturating_sub(1));
    }
}

/// A `TcpStream` preceded by already-read bytes, used when handing a connection
/// off from the lean HTTP/1.1 path to Hyper (`h2c`, WebSocket upgrades, chunked
/// or large bodies, `100-continue`).
pub(crate) struct PrefixedIo {
    prefix: Vec<u8>,
    offset: usize,
    inner: TcpStream,
}

impl PrefixedIo {
    fn new(prefix: Vec<u8>, inner: TcpStream) -> Self {
        Self {
            prefix,
            offset: 0,
            inner,
        }
    }
}

impl AsyncRead for PrefixedIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if self.offset < self.prefix.len() {
            let remaining = &self.prefix[self.offset..];
            let count = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..count]);
            self.offset += count;
            if self.offset == self.prefix.len() {
                // Release the prefix buffer as soon as Hyper has drained it.
                self.prefix = Vec::new();
                self.offset = 0;
            }
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for PrefixedIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

/// Boxed off the hot `serve_connection` future so Hyper's ~18 KiB HTTP/1+HTTP/2
/// state machine is never allocated for ordinary HTTP/1.1 keep-alive sockets.
#[cold]
fn handoff_to_hyper(
    buf: Vec<u8>,
    stream: TcpStream,
    peer: SocketAddr,
    shutdown: watch::Receiver<bool>,
    work: Rc<ConnectionWork>,
    worker: &Rc<WorkerContext>,
) -> Pin<Box<dyn Future<Output = ()>>> {
    let params = ConnectionParams {
        io: PrefixedIo::new(buf, stream),
        peer,
        local_addr: worker.local_addr,
        handler: worker.handler.clone(),
        shutdown,
        limits: worker.limits,
        work,
        ws: worker.ws.clone(),
    };
    Box::pin(handle_hyper_connection(params))
}

/// Serve one TCP connection on the zero-idle-buffer HTTP/1.1 fast path,
/// transparently delegating HTTP/2, WebSockets, and complex HTTP/1.1 framing
/// to Hyper via a boxed cold handoff.
pub(crate) async fn serve_connection(
    mut stream: TcpStream,
    peer: SocketAddr,
    shutdown: watch::Receiver<bool>,
    work: Rc<ConnectionWork>,
    worker: Rc<WorkerContext>,
) {
    let _active = ActiveConnectionGuard::enter(worker.clone());
    let limits = worker.limits;
    let read_limit = limits.idle_timeout.min(limits.request_read_timeout);
    let mut first_request = true;
    let mut pending: Option<Vec<u8>> = None;

    // Pin ONE shutdown waiter per connection so keep-alive requests never pay
    // per-request `watch::changed` subscribe/unsubscribe overhead.
    let mut shutdown_waiter = shutdown.clone();
    let shutdown_fut = wait_for_shutdown(&mut shutdown_waiter);
    tokio::pin!(shutdown_fut);

    loop {
        if *shutdown.borrow() {
            return;
        }

        // Step 1: Try an immediate non-blocking read before arming an idle
        // timer or polling shutdown. When no bytes are ready, release the
        // scratch buffer immediately and park with ZERO heap buffer held.
        let mut buf = if let Some(leftover) = pending.take() {
            leftover
        } else {
            let mut scratch_buf = worker.scratch.take_read_buf();
            match stream.try_read_buf(&mut scratch_buf) {
                Ok(0) => {
                    worker.scratch.return_read_buf(scratch_buf);
                    return;
                }
                Ok(_) => scratch_buf,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    worker.scratch.return_read_buf(scratch_buf);
                    let idle_ready = tokio::select! {
                        biased;
                        ready = tokio::time::timeout(read_limit, stream.readable()) => ready,
                        _ = &mut shutdown_fut => return,
                    };
                    match idle_ready {
                        Ok(Ok(())) => worker.scratch.take_read_buf(),
                        Ok(Err(_)) | Err(_) => return,
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    worker.scratch.return_read_buf(scratch_buf);
                    continue;
                }
                Err(_) => {
                    worker.scratch.return_read_buf(scratch_buf);
                    return;
                }
            }
        };

        let head_started = Instant::now();
        let head_deadline = head_started + read_limit;

        // Step 2: Read and parse the HTTP/1.x request head (or sniff HTTP/2 preface).
        let parsed = loop {
            if buf.is_empty() {
                match read_chunk(&mut stream, &mut buf, head_deadline, shutdown_fut.as_mut()).await
                {
                    ReadOutcome::GotBytes => {}
                    ReadOutcome::Eof
                    | ReadOutcome::Shutdown
                    | ReadOutcome::IoError
                    | ReadOutcome::TimedOut => {
                        worker.scratch.return_read_buf(buf);
                        return;
                    }
                }
            }

            if first_request && starts_like_h2_preface(&buf) {
                if buf.len() >= H2_PREFACE.len() {
                    handoff_to_hyper(buf, stream, peer, shutdown, work, &worker).await;
                    return;
                }
                match read_chunk(&mut stream, &mut buf, head_deadline, shutdown_fut.as_mut()).await
                {
                    ReadOutcome::GotBytes => continue,
                    // Partial HTTP/2 preface that stalls must close silently
                    // without sending an HTTP/1.1 408 response.
                    ReadOutcome::Eof
                    | ReadOutcome::Shutdown
                    | ReadOutcome::IoError
                    | ReadOutcome::TimedOut => {
                        worker.scratch.return_read_buf(buf);
                        return;
                    }
                }
            }

            match parse_head_meta(&buf) {
                ParseResult::Complete(info) => break info,
                ParseResult::Partial => {
                    if buf.len() >= MAX_FAST_HEAD_BYTES {
                        handoff_to_hyper(buf, stream, peer, shutdown, work, &worker).await;
                        return;
                    }
                    match read_chunk(&mut stream, &mut buf, head_deadline, shutdown_fut.as_mut())
                        .await
                    {
                        ReadOutcome::GotBytes => continue,
                        ReadOutcome::TimedOut => {
                            worker.scratch.return_read_buf(buf);
                            if limits.request_read_timeout <= limits.idle_timeout {
                                const TIMEOUT_408: &[u8] = b"HTTP/1.1 408 Request Timeout\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
                                let _ = write_error_and_close(&mut stream, TIMEOUT_408).await;
                            }
                            return;
                        }
                        ReadOutcome::Eof | ReadOutcome::Shutdown | ReadOutcome::IoError => {
                            worker.scratch.return_read_buf(buf);
                            return;
                        }
                    }
                }
                ParseResult::FallbackToHyper => {
                    handoff_to_hyper(buf, stream, peer, shutdown, work, &worker).await;
                    return;
                }
                ParseResult::BadRequest => {
                    worker.scratch.return_read_buf(buf);
                    const BAD_400: &[u8] =
                        b"HTTP/1.1 400 Bad Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n";
                    let _ = write_error_and_close(&mut stream, BAD_400).await;
                    return;
                }
            }
        };

        first_request = false;

        // Check body size & framing fallback before reading body bytes.
        let content_length = parsed.content_length;
        if content_length > limits.max_body_bytes || content_length > MAX_FAST_BODY_BYTES {
            handoff_to_hyper(buf, stream, peer, shutdown, work, &worker).await;
            return;
        }

        // Step 3: Read any remaining fixed-length body bytes under the shared
        // head+body budget and body_idle_timeout.
        let total_needed = parsed.head_len + content_length;
        if buf.len() < total_needed {
            buf.reserve(total_needed - buf.len());
            while buf.len() < total_needed {
                let body_so_far = buf.len() - parsed.head_len;
                let total_budget = limits
                    .request_read_timeout
                    .saturating_add(body_time_credit(body_so_far));
                let remaining = total_budget.saturating_sub(head_started.elapsed());
                if remaining.is_zero() {
                    worker.scratch.return_read_buf(buf);
                    let _ = write_stalled_408(&mut stream).await;
                    return;
                }
                let step_deadline = Instant::now() + remaining.min(limits.body_idle_timeout);
                match read_chunk(&mut stream, &mut buf, step_deadline, shutdown_fut.as_mut()).await
                {
                    ReadOutcome::GotBytes => {}
                    ReadOutcome::TimedOut => {
                        worker.scratch.return_read_buf(buf);
                        let _ = write_stalled_408(&mut stream).await;
                        return;
                    }
                    ReadOutcome::Eof | ReadOutcome::Shutdown | ReadOutcome::IoError => {
                        worker.scratch.return_read_buf(buf);
                        return;
                    }
                }
            }
        }

        // Step 4: Construct Request and return the scratch read buffer BEFORE
        // awaiting the PHP handler (so slow async handlers hold zero read buffers).
        let is_head = parsed.is_head;
        let version = parsed.version;
        let client_keep_alive = match version {
            HttpVersion::Http11 => !parsed.connection_close,
            HttpVersion::Http10 => parsed.connection_keep_alive,
            HttpVersion::Http2 => false,
        };
        let request = build_request(&buf, parsed, peer, worker.local_addr);
        if buf.len() > total_needed {
            pending = Some(buf[total_needed..].to_vec());
        }
        worker.scratch.return_read_buf(buf);

        // Step 5: Execute the handler via `answer` (uses Phase 1 single-poll
        // sync fast path + detached/streaming/timeout handling).
        // `biased;` polls `answer` first so synchronous handlers never even
        // poll `wait_peer_closed`; async handlers observe a client disconnect
        // promptly and detach cleanly via `ResponseTask::drop`.
        let mut response = if pending.is_none() {
            tokio::select! {
                biased;
                response = answer(
                    worker.handler.clone(),
                    request,
                    limits.request_timeout,
                    work.clone(),
                ) => response,
                _ = wait_peer_closed(&stream) => return,
            }
        } else {
            answer(
                worker.handler.clone(),
                request,
                limits.request_timeout,
                work.clone(),
            )
            .await
        };
        if response.upgrade.take().is_some() {
            response = UpgradeRejection::UpgradeRequired.into_response();
        }

        // Step 6: Decide keep-alive (respecting client, handler, shutdown, and
        // connection-pressure load shedding).
        let shedding = limits
            .keepalive_shed_at
            .is_some_and(|threshold| worker.active_connections.get() >= threshold);
        let handler_closed = response.headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("connection") && contains_token_ascii_ci(value, b"close")
        });
        let keep_alive = client_keep_alive && !handler_closed && !shedding && !*shutdown.borrow();

        // Step 7: Serialize and write the HTTP/1.1 response directly.
        let write_ok = write_response(
            &mut stream,
            response,
            is_head,
            version,
            keep_alive,
            pending.is_none(),
            &worker.scratch,
        )
        .await;
        if !write_ok || !keep_alive {
            let _ = stream.shutdown().await;
            return;
        }
    }
}

#[inline]
fn starts_like_h2_preface(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(H2_PREFACE.len());
    bytes[..check_len] == H2_PREFACE[..check_len]
}

enum ReadOutcome {
    GotBytes,
    Eof,
    TimedOut,
    Shutdown,
    IoError,
}

async fn read_chunk<F: Future<Output = ()>>(
    stream: &mut TcpStream,
    buf: &mut Vec<u8>,
    deadline: Instant,
    mut shutdown_fut: Pin<&mut F>,
) -> ReadOutcome {
    loop {
        match stream.try_read_buf(buf) {
            Ok(0) => return ReadOutcome::Eof,
            Ok(_) => return ReadOutcome::GotBytes,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return ReadOutcome::IoError,
        }
        let waited = tokio::select! {
            biased;
            ready = tokio::time::timeout_at(deadline, stream.readable()) => ready,
            _ = shutdown_fut.as_mut() => return ReadOutcome::Shutdown,
        };
        match waited {
            Ok(Ok(())) => {}
            Ok(Err(_)) => return ReadOutcome::IoError,
            Err(_) => return ReadOutcome::TimedOut,
        }
    }
}

/// Compact `Copy` metadata extracted from a complete HTTP/1.x request head.
/// Zero heap allocations and tiny enough to live across body reads without
/// bloating the connection future.
#[derive(Clone, Copy)]
struct HeadMeta {
    head_len: usize,
    content_length: usize,
    version: HttpVersion,
    connection_close: bool,
    connection_keep_alive: bool,
    is_head: bool,
}

enum ParseResult {
    Complete(HeadMeta),
    Partial,
    FallbackToHyper,
    BadRequest,
}

fn parse_head_meta(buf: &[u8]) -> ParseResult {
    let mut raw_headers = [httparse::EMPTY_HEADER; MAX_FAST_HEADERS];
    let mut req = httparse::Request::new(&mut raw_headers);
    let head_len = match req.parse(buf) {
        Ok(httparse::Status::Complete(len)) => len,
        Ok(httparse::Status::Partial) => return ParseResult::Partial,
        Err(httparse::Error::TooManyHeaders) => return ParseResult::FallbackToHyper,
        Err(_) => return ParseResult::BadRequest,
    };

    let Some(method_str) = req.method else {
        return ParseResult::BadRequest;
    };
    let Some(target_str) = req.path else {
        return ParseResult::BadRequest;
    };
    let version = match req.version {
        Some(1) => HttpVersion::Http11,
        Some(0) => HttpVersion::Http10,
        _ => return ParseResult::FallbackToHyper,
    };

    // Origin-form targets (`/path` or `/path?query`) cover 99.99% of traffic.
    // Authority-form (`CONNECT`), asterisk-form (`*`), or absolute-form URIs
    // (`http://host/path`) hand off to Hyper so authority/Host handling matches.
    if !target_str.starts_with('/') {
        return ParseResult::FallbackToHyper;
    }

    let mut content_length: Option<usize> = None;
    let mut has_host = false;
    let mut connection_close = false;
    let mut connection_keep_alive = false;

    for header in req.headers.iter() {
        let name = header.name;
        let value = trim_ows(header.value);
        if name.eq_ignore_ascii_case("transfer-encoding")
            || name.eq_ignore_ascii_case("upgrade")
            || name.eq_ignore_ascii_case("expect")
        {
            return ParseResult::FallbackToHyper;
        }
        if name.eq_ignore_ascii_case("host") {
            if has_host {
                return ParseResult::BadRequest;
            }
            has_host = true;
        } else if name.eq_ignore_ascii_case("content-length") {
            let Ok(text) = std::str::from_utf8(value) else {
                return ParseResult::BadRequest;
            };
            let Ok(parsed_len) = text.parse::<usize>() else {
                return ParseResult::BadRequest;
            };
            if let Some(prev) = content_length
                && prev != parsed_len
            {
                return ParseResult::BadRequest;
            }
            content_length = Some(parsed_len);
        } else if name.eq_ignore_ascii_case("connection") {
            if contains_token_ascii_ci(value, b"close") {
                connection_close = true;
            }
            if contains_token_ascii_ci(value, b"keep-alive") {
                connection_keep_alive = true;
            }
            if contains_token_ascii_ci(value, b"upgrade") {
                return ParseResult::FallbackToHyper;
            }
        }
    }

    if version == HttpVersion::Http11 && !has_host {
        return ParseResult::BadRequest;
    }

    ParseResult::Complete(HeadMeta {
        head_len,
        content_length: content_length.unwrap_or(0),
        version,
        connection_close,
        connection_keep_alive,
        is_head: method_str == "HEAD",
    })
}

fn build_request(buf: &[u8], meta: HeadMeta, peer: SocketAddr, local_addr: SocketAddr) -> Request {
    let mut raw_headers = [httparse::EMPTY_HEADER; MAX_FAST_HEADERS];
    let mut req = httparse::Request::new(&mut raw_headers);
    let _ = req.parse(&buf[..meta.head_len]);
    let method_str = req.method.unwrap_or("GET");
    let target_str = req.path.unwrap_or("/");
    let (path, query) = match target_str.split_once('?') {
        Some((p, q)) => (p.to_owned(), q.to_owned()),
        None => (target_str.to_owned(), String::new()),
    };
    let target = if query.is_empty() && target_str.len() == path.len() {
        path.clone()
    } else {
        target_str.to_owned()
    };
    let mut headers = Vec::with_capacity(req.headers.len());
    for header in req.headers.iter() {
        let lower = header.name.to_ascii_lowercase();
        headers.push((lower, trim_ows(header.value).to_vec()));
    }
    let body = if meta.content_length == 0 {
        Vec::new()
    } else {
        buf[meta.head_len..meta.head_len + meta.content_length].to_vec()
    };
    Request {
        method: method_str.to_owned(),
        target,
        path,
        query,
        version: meta.version,
        headers,
        body,
        remote_addr: peer,
        local_addr,
        is_websocket_upgrade: false,
    }
}

fn trim_ows(mut bytes: &[u8]) -> &[u8] {
    while let Some(&first) = bytes.first() {
        if first == b' ' || first == b'\t' {
            bytes = &bytes[1..];
        } else {
            break;
        }
    }
    while let Some(&last) = bytes.last() {
        if last == b' ' || last == b'\t' {
            bytes = &bytes[..bytes.len() - 1];
        } else {
            break;
        }
    }
    bytes
}

fn contains_token_ascii_ci(value: &[u8], token: &[u8]) -> bool {
    value
        .split(|&b| b == b',')
        .any(|part| trim_ows(part).eq_ignore_ascii_case(token))
}

fn valid_header_name(name: &str) -> bool {
    !name.is_empty()
        && name.bytes().all(|b| {
            matches!(
                b,
                b'!' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'*' | b'+' | b'-' | b'.' | b'^'
                | b'_' | b'`' | b'|' | b'~' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z'
            )
        })
}

fn valid_header_value(value: &[u8]) -> bool {
    value
        .iter()
        .all(|&b| b == b'\t' || (b >= 0x20 && b != 0x7f))
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        412 => "Precondition Failed",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        416 => "Range Not Satisfiable",
        422 => "Unprocessable Content",
        426 => "Upgrade Required",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

#[derive(Clone, Copy)]
struct HeadFraming {
    content_length: Option<u64>,
    chunked: bool,
    version: HttpVersion,
    keep_alive: bool,
}

fn write_status_and_headers(
    out: &mut Vec<u8>,
    status: u16,
    headers: &[(String, Vec<u8>)],
    framing: HeadFraming,
    scratch: &WorkerScratch,
) {
    let status = if (100..=999).contains(&status) {
        status
    } else {
        500
    };
    let is_switching_protocols = status == 101;
    let mut num = itoa::Buffer::new();
    out.extend_from_slice(b"HTTP/1.1 ");
    out.extend_from_slice(num.format(status).as_bytes());
    let reason = reason_phrase(status);
    if !reason.is_empty() {
        out.push(b' ');
        out.extend_from_slice(reason.as_bytes());
    }
    out.extend_from_slice(b"\r\n");

    let mut has_date = false;
    let mut has_connection = false;
    for (name, value) in headers {
        if !valid_header_name(name) || !valid_header_value(value) {
            continue;
        }
        if !is_switching_protocols
            && (name.eq_ignore_ascii_case("content-length")
                || name.eq_ignore_ascii_case("transfer-encoding"))
        {
            continue;
        }
        if name.eq_ignore_ascii_case("date") {
            has_date = true;
        } else if name.eq_ignore_ascii_case("connection") {
            if !framing.keep_alive {
                // We will emit a clean `connection: close` below.
                continue;
            }
            has_connection = true;
        }
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(value);
        out.extend_from_slice(b"\r\n");
    }

    if framing.chunked {
        out.extend_from_slice(b"transfer-encoding: chunked\r\n");
    } else if let Some(len) = framing.content_length {
        out.extend_from_slice(b"content-length: ");
        out.extend_from_slice(num.format(len).as_bytes());
        out.extend_from_slice(b"\r\n");
    }

    if !framing.keep_alive {
        out.extend_from_slice(b"connection: close\r\n");
    } else if framing.version == HttpVersion::Http10 && !has_connection {
        out.extend_from_slice(b"connection: keep-alive\r\n");
    }

    if !has_date {
        scratch.write_date_header(out);
    }
    out.extend_from_slice(b"\r\n");
}

async fn write_all_fast(stream: &mut TcpStream, bytes: &[u8]) -> bool {
    let mut written = 0usize;
    while written < bytes.len() {
        match stream.try_write(&bytes[written..]) {
            Ok(0) => return false,
            Ok(n) => written += n,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if stream.write_all(&bytes[written..]).await.is_err() {
                    return false;
                }
                return true;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return false,
        }
    }
    true
}

#[cold]
fn write_error_and_close<'a>(
    stream: &'a mut TcpStream,
    bytes: &'static [u8],
) -> Pin<Box<dyn Future<Output = bool> + 'a>> {
    Box::pin(async move {
        tokio::time::timeout(Duration::from_secs(1), stream.write_all(bytes))
            .await
            .is_ok_and(|res| res.is_ok())
    })
}

#[cold]
fn write_stalled_408<'a>(stream: &'a mut TcpStream) -> Pin<Box<dyn Future<Output = bool> + 'a>> {
    const STALLED_408: &[u8] = b"HTTP/1.1 408 Request Timeout\r\ncontent-type: text/plain; charset=utf-8\r\nconnection: close\r\ncontent-length: 21\r\n\r\nrequest read timeout\n";
    write_error_and_close(stream, STALLED_408)
}

/// Resolves once the peer closes its half of the TCP connection (EOF or reset).
/// Peeks without consuming bytes; if pipelined HTTP/1.1 bytes are buffered,
/// stops polling so the current response can still finish cleanly.
async fn wait_peer_closed(stream: &TcpStream) {
    let mut probe = [0u8; 1];
    match stream.peek(&mut probe).await {
        Ok(0) | Err(_) => {}
        Ok(_) => std::future::pending::<()>().await,
    }
}

async fn write_response(
    stream: &mut TcpStream,
    response: Response,
    is_head: bool,
    version: HttpVersion,
    keep_alive: bool,
    watch_disconnect: bool,
    scratch: &WorkerScratch,
) -> bool {
    let status = response.status;
    let bodyless_status = (100..200).contains(&status) || status == 204 || status == 304;
    let omit_body = is_head || bodyless_status;

    match response.body {
        Body::Bytes(body_bytes) => {
            let content_length = if bodyless_status {
                if status == 304 && !body_bytes.is_empty() {
                    Some(body_bytes.len() as u64)
                } else {
                    None
                }
            } else {
                Some(body_bytes.len() as u64)
            };
            let mut out = scratch.take_write_buf();
            write_status_and_headers(
                &mut out,
                status,
                &response.headers,
                HeadFraming {
                    content_length,
                    chunked: false,
                    version,
                    keep_alive,
                },
                scratch,
            );
            let ok = if omit_body || body_bytes.is_empty() {
                write_all_fast(stream, &out).await
            } else if out.len() + body_bytes.len() <= 64 * 1024 {
                out.extend_from_slice(&body_bytes);
                write_all_fast(stream, &out).await
            } else {
                write_all_fast(stream, &out).await && write_all_fast(stream, &body_bytes).await
            };
            scratch.return_write_buf(out);
            ok
        }
        other => {
            write_non_bytes_response(
                stream,
                status,
                response.headers,
                other,
                bodyless_status,
                omit_body,
                version,
                keep_alive,
                watch_disconnect,
                scratch,
            )
            .await
        }
    }
}

#[cold]
#[allow(clippy::too_many_arguments)]
fn write_non_bytes_response<'a>(
    stream: &'a mut TcpStream,
    status: u16,
    headers: Vec<(String, Vec<u8>)>,
    body: Body,
    bodyless_status: bool,
    omit_body: bool,
    version: HttpVersion,
    keep_alive: bool,
    watch_disconnect: bool,
    scratch: &'a WorkerScratch,
) -> Pin<Box<dyn Future<Output = bool> + 'a>> {
    Box::pin(async move {
        match body {
            Body::Bytes(_) => true,
            Body::File(slice) => {
                let content_length = (!bodyless_status).then_some(slice.length);
                let mut out = scratch.take_write_buf();
                write_status_and_headers(
                    &mut out,
                    status,
                    &headers,
                    HeadFraming {
                        content_length,
                        chunked: false,
                        version,
                        keep_alive,
                    },
                    scratch,
                );
                let head_ok = write_all_fast(stream, &out).await;
                scratch.return_write_buf(out);
                if !head_ok {
                    return false;
                }
                if omit_body || slice.length == 0 {
                    return true;
                }
                let mut file = File::from_std(slice.file);
                let mut remaining = slice.length;
                let mut chunk = vec![0u8; remaining.min(FILE_CHUNK_BYTES as u64) as usize];
                while remaining > 0 {
                    let want = remaining.min(chunk.len() as u64) as usize;
                    let read = match file.read(&mut chunk[..want]).await {
                        Ok(0) => return false,
                        Ok(n) => n,
                        Err(_) => return false,
                    };
                    if stream.write_all(&chunk[..read]).await.is_err() {
                        return false;
                    }
                    remaining -= read as u64;
                }
                true
            }
            Body::Stream(mut stream_body) => {
                let chunked = !omit_body && version == HttpVersion::Http11;
                let mut out = scratch.take_write_buf();
                write_status_and_headers(
                    &mut out,
                    status,
                    &headers,
                    HeadFraming {
                        content_length: None,
                        chunked,
                        version,
                        keep_alive: keep_alive && chunked,
                    },
                    scratch,
                );
                let head_ok = write_all_fast(stream, &out).await;
                scratch.return_write_buf(out);
                if !head_ok {
                    return false;
                }
                if omit_body {
                    return true;
                }
                loop {
                    let next = if watch_disconnect {
                        tokio::select! {
                            biased;
                            next = poll_fn(|cx| stream_body.poll_chunk(cx)) => next,
                            _ = wait_peer_closed(stream) => return false,
                        }
                    } else {
                        poll_fn(|cx| stream_body.poll_chunk(cx)).await
                    };
                    let Some(next) = next else {
                        break;
                    };
                    let Ok(bytes) = next else {
                        return false;
                    };
                    if bytes.is_empty() {
                        continue;
                    }
                    if chunked {
                        let header = format!("{:x}\r\n", bytes.len());
                        if stream.write_all(header.as_bytes()).await.is_err()
                            || stream.write_all(&bytes).await.is_err()
                            || stream.write_all(b"\r\n").await.is_err()
                        {
                            return false;
                        }
                    } else if stream.write_all(&bytes).await.is_err() {
                        return false;
                    }
                }
                if chunked {
                    stream.write_all(b"0\r\n\r\n").await.is_ok()
                } else {
                    false
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Guard against accidentally embedding Hyper's ~18 KiB `auto::Connection`
    /// state machine back inside `serve_connection`'s async future.
    #[test]
    fn serve_connection_future_stays_compact() {
        fn future_size<A, B, C, D, E, F: Future>(_: fn(A, B, C, D, E) -> F) -> usize {
            std::mem::size_of::<F>()
        }
        let size = future_size(serve_connection);
        assert!(
            size <= 2048,
            "serve_connection future grew to {size} bytes (expected <= 2048)"
        );
    }
}
