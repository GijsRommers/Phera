//! WebSockets over HTTP/2 extended CONNECT (RFC 8441) against a real
//! `phera_server::Server`: the SETTINGS the server announces, the handshake,
//! echo, rooms shared with HTTP/1.1 connections, several WebSocket streams
//! and ordinary requests on one connection, back pressure in both
//! directions, close codes, limits, and `onClose` when the connection drops.

mod common;

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Empty};
use hyper::client::conn::http2;
use hyper::ext::Protocol;
use hyper::header::HeaderMap;
use hyper::{Method, Request as HyperRequest};
use hyper_util::rt::{TokioExecutor, TokioIo};
use phera_server::{
    Handler, HandlerFactory, Request, Response, Server, ServerConfig, ServerHandle,
    WebSocketConfig, WebSocketEvent, WebSocketEventKind, WebSocketHub, WebSocketOutcome,
    WebSocketUpgrade,
};
use tokio::net::TcpStream;
use tokio::sync::Semaphore;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, Role};

type Log = Arc<Mutex<Vec<String>>>;
type H2WebSocket = WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>;

struct Http2TestFactory {
    hub: Arc<WebSocketHub>,
    log: Log,
    /// `stuck` handlers wait here until the test lets them go.
    release: Arc<Semaphore>,
}

impl HandlerFactory for Http2TestFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(Http2TestHandler {
            hub: self.hub.clone(),
            log: self.log.clone(),
            release: self.release.clone(),
        })
    }
}

struct Http2TestHandler {
    hub: Arc<WebSocketHub>,
    log: Log,
    release: Arc<Semaphore>,
}

fn accept(entry: &str, rooms: &[&str]) -> Response {
    Response::new(101)
        .with_header("set-cookie", "ws_session=ok")
        .with_websocket_upgrade(WebSocketUpgrade {
            entry: entry.to_string(),
            handler_class: entry.to_string(),
            subprotocol: Some("chat.v1".to_string()),
            context: Vec::new(),
            rooms: rooms.iter().map(|room| room.to_string()).collect(),
        })
}

impl Handler for Http2TestHandler {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async move {
            if request.is_websocket_upgrade {
                // What PHP would see in `$_SERVER`.
                let header = |name| {
                    String::from_utf8_lossy(request.header(name).unwrap_or_default()).into_owned()
                };
                self.log.lock().unwrap().push(format!(
                    "request:{} {} {:?} upgrade={} connection={} host={}",
                    request.method,
                    request.path,
                    request.version,
                    header("upgrade"),
                    header("connection"),
                    header("host"),
                ));
            }
            match request.path.as_str() {
                "/deny" => Response::text(401, "unauthorized\n"),
                "/echo" => accept("echo", &[]),
                "/chat" => accept("chat", &["lobby"]),
                "/stuck" => accept("stuck", &[]),
                _ => Response::text(200, "hello\n"),
            }
        })
    }

    fn handle_websocket(
        &self,
        event: WebSocketEvent,
    ) -> Pin<Box<dyn Future<Output = WebSocketOutcome> + '_>> {
        Box::pin(async move {
            let id = event.connection_id.clone();
            match (&*event.entry, event.kind) {
                (entry, WebSocketEventKind::Open) => {
                    self.log.lock().unwrap().push(format!("{entry}:open:{id}"));
                }
                ("echo", WebSocketEventKind::Message { data, binary }) => {
                    self.hub.send(&id, &data, binary);
                }
                ("chat", WebSocketEventKind::Message { data, binary }) => {
                    self.hub.broadcast("lobby", &data, binary, Some(&id));
                }
                ("stuck", WebSocketEventKind::Message { .. }) => {
                    // A PHP handler that falls behind: its stream stops being read.
                    self.log.lock().unwrap().push("stuck:message".to_string());
                    let _ = self.release.acquire().await;
                }
                (entry, WebSocketEventKind::Close { code, reason }) => {
                    self.log
                        .lock()
                        .unwrap()
                        .push(format!("{entry}:close:{code}:{reason}"));
                }
                _ => {}
            }
            WebSocketOutcome::default()
        })
    }
}

struct TestServer {
    handle: ServerHandle,
    addr: SocketAddr,
    hub: Arc<WebSocketHub>,
    log: Log,
    release: Arc<Semaphore>,
}

impl TestServer {
    fn start(websocket: WebSocketConfig) -> Self {
        let config = ServerConfig {
            websocket,
            ..common::test_config()
        };
        let hub = Arc::new(WebSocketHub::new(
            config.websocket.send_queue_len,
            config.websocket.send_queue_bytes,
        ));
        let log = Log::default();
        let release = Arc::new(Semaphore::new(0));
        let factory = Http2TestFactory {
            hub: hub.clone(),
            log: log.clone(),
            release: release.clone(),
        };
        let server = Server::bind_with_hub(config, factory, hub.clone()).unwrap();
        let addr = server.local_addr();
        Self {
            handle: server.spawn(),
            addr,
            hub,
            log,
            release,
        }
    }

    fn logged(&self, prefix: &str) -> Vec<String> {
        let log = self.log.lock().unwrap();
        log.iter()
            .filter(|line| line.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// Wait until `count` log lines start with `prefix`.
    async fn wait_logged(&self, prefix: &str, count: usize) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let lines = self.logged(prefix);
            if lines.len() >= count || Instant::now() > deadline {
                return lines;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Shut the server down while this test's own runtime keeps running, so
    /// its clients can answer the server's GOAWAY and close frames.
    fn begin_stop(self) -> tokio::task::JoinHandle<()> {
        self.release.add_permits(1_000);
        tokio::task::spawn_blocking(move || self.handle.shutdown())
    }

    async fn stop(self) {
        self.begin_stop().await.unwrap();
    }
}

/// One HTTP/2 connection (`h2c` with prior knowledge) from hyper's client.
struct H2Client {
    addr: SocketAddr,
    sender: http2::SendRequest<Empty<Bytes>>,
}

impl H2Client {
    async fn connect(addr: SocketAddr) -> Self {
        Self::connect_with(addr, http2::Builder::new(TokioExecutor::new())).await
    }

    async fn connect_with(addr: SocketAddr, builder: http2::Builder<TokioExecutor>) -> Self {
        let tcp = TcpStream::connect(addr).await.unwrap();
        let (sender, connection) = builder.handshake(TokioIo::new(tcp)).await.unwrap();
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let mut client = Self { addr, sender };
        // A client may send `:protocol` only after the server's SETTINGS
        // arrived; one answered request guarantees that.
        assert_eq!(client.get("/hello").await, (200, "hello\n".to_string()));
        client
    }

    async fn get(&mut self, path: &str) -> (u16, String) {
        let request = HyperRequest::get(format!("http://{}{path}", self.addr))
            .body(Empty::new())
            .unwrap();
        self.sender.ready().await.unwrap();
        let response = self.sender.send_request(request).await.unwrap();
        let status = response.status().as_u16();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    /// Send an extended CONNECT; `Err` carries the status of a refusal.
    async fn open(
        &mut self,
        path: &str,
        protocol: &'static str,
        headers: &[(&str, &str)],
    ) -> Result<(H2WebSocket, HeaderMap), u16> {
        let mut builder = HyperRequest::builder()
            .method(Method::CONNECT)
            .uri(format!("http://{}{path}", self.addr))
            .header("sec-websocket-protocol", "chat.v1");
        if !headers
            .iter()
            .any(|(name, _)| *name == "sec-websocket-version")
        {
            builder = builder.header("sec-websocket-version", "13");
        }
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let mut request = builder.body(Empty::new()).unwrap();
        request
            .extensions_mut()
            .insert(Protocol::from_static(protocol));
        self.sender.ready().await.unwrap();
        let response = self.sender.send_request(request).await.unwrap();
        if response.status() != 200 {
            return Err(response.status().as_u16());
        }
        let headers = response.headers().clone();
        let upgraded = hyper::upgrade::on(response).await.unwrap();
        let ws = WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Client, None).await;
        Ok((ws, headers))
    }

    async fn websocket(&mut self, path: &str) -> H2WebSocket {
        self.open(path, "websocket", &[]).await.unwrap().0
    }
}

/// The h2 crate's client, whose connection task owns the TCP socket: aborting
/// the task closes the socket under every stream at once.
struct RawH2;

impl RawH2 {
    async fn connect(
        addr: SocketAddr,
    ) -> (h2::client::SendRequest<Bytes>, tokio::task::JoinHandle<()>) {
        let tcp = TcpStream::connect(addr).await.unwrap();
        let (client, connection) = h2::client::handshake(tcp).await.unwrap();
        let connection = tokio::spawn(async move {
            let _ = connection.await;
        });
        let mut client = client.ready().await.unwrap();
        let request = HyperRequest::get(format!("http://{addr}/hello"))
            .body(())
            .unwrap();
        let (response, _) = client.send_request(request, true).unwrap();
        assert_eq!(response.await.unwrap().status(), 200);
        (client, connection)
    }

    /// Open a WebSocket stream without speaking WebSocket on it.
    async fn open(
        client: &mut h2::client::SendRequest<Bytes>,
        addr: SocketAddr,
        path: &str,
    ) -> h2::SendStream<Bytes> {
        let mut request = HyperRequest::builder()
            .method(Method::CONNECT)
            .uri(format!("http://{addr}{path}"))
            .header("sec-websocket-version", "13")
            .body(())
            .unwrap();
        request
            .extensions_mut()
            .insert(h2::ext::Protocol::from_static("websocket"));
        let (response, stream) = client.send_request(request, false).unwrap();
        assert_eq!(response.await.unwrap().status(), 200);
        stream
    }
}

async fn next_message(ws: &mut H2WebSocket) -> Message {
    tokio::time::timeout(Duration::from_secs(5), ws.next())
        .await
        .expect("a message within five seconds")
        .expect("the stream is open")
        .expect("a valid frame")
}

fn close_code(message: Message) -> u16 {
    match message {
        Message::Close(Some(frame)) => u16::from(frame.code),
        other => panic!("expected a close frame, got {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn server_announces_extended_connect_and_refuses_what_it_does_not_speak() {
    let server = TestServer::start(WebSocketConfig::default());

    // The SETTINGS frame as the h2 crate's own client reads it.
    let (client, _connection) = RawH2::connect(server.addr).await;
    assert!(client.is_extended_connect_protocol_enabled());

    let mut h2 = H2Client::connect(server.addr).await;
    // Another protocol over extended CONNECT, and a WebSocket version that
    // does not exist, are refused before PHP runs.
    assert_eq!(h2.open("/echo", "webtransport", &[]).await.err(), Some(501));
    let wrong_version = [("sec-websocket-version", "8")];
    assert_eq!(
        h2.open("/echo", "websocket", &wrong_version).await.err(),
        Some(400)
    );
    // The handler's own refusal reaches the client unchanged.
    assert_eq!(h2.open("/deny", "websocket", &[]).await.err(), Some(401));
    // A page that answers 200 without accepting must not open the stream:
    // on HTTP/2 a 2xx would mean the WebSocket is open.
    assert_eq!(h2.open("/page", "websocket", &[]).await.err(), Some(400));
    // The connection is still usable for ordinary requests afterwards.
    assert_eq!(h2.get("/page").await, (200, "hello\n".to_string()));
    assert!(server.logged("echo:open").is_empty());
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn echo_over_an_http2_stream_looks_like_an_http1_upgrade_to_the_handler() {
    let server = TestServer::start(WebSocketConfig::default());
    let mut h2 = H2Client::connect(server.addr).await;

    let (mut ws, headers) = h2.open("/echo", "websocket", &[]).await.unwrap();
    // Headers PHP set before accept() and the negotiated subprotocol arrive;
    // the HTTP/1.1-only handshake headers do not.
    assert_eq!(headers["set-cookie"], "ws_session=ok");
    assert_eq!(headers["sec-websocket-protocol"], "chat.v1");
    assert!(headers.get("sec-websocket-accept").is_none());
    assert!(headers.get("upgrade").is_none());

    ws.send(Message::text("hello")).await.unwrap();
    assert_eq!(next_message(&mut ws).await, Message::text("hello"));
    ws.send(Message::binary(vec![0, 1, 2, 255])).await.unwrap();
    assert_eq!(
        next_message(&mut ws).await,
        Message::binary(vec![0, 1, 2, 255])
    );

    let addr = server.addr;
    assert_eq!(
        server.logged("request:"),
        [format!(
            "request:GET /echo Http2 upgrade=websocket connection=Upgrade host={addr}"
        )]
    );

    // Shutdown sends 1001 on the stream, as on an HTTP/1.1 socket.
    let stopping = server.begin_stop();
    assert_eq!(close_code(next_message(&mut ws).await), 1001);
    drop((ws, h2));
    stopping.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn two_websocket_streams_and_ordinary_requests_share_one_connection() {
    let server = TestServer::start(WebSocketConfig::default());
    let mut h2 = H2Client::connect(server.addr).await;

    let mut first = h2.websocket("/echo").await;
    let mut second = h2.websocket("/echo").await;
    first.send(Message::text("one")).await.unwrap();
    second.send(Message::text("two")).await.unwrap();
    assert_eq!(h2.get("/hello").await.0, 200);
    assert_eq!(next_message(&mut second).await, Message::text("two"));
    assert_eq!(next_message(&mut first).await, Message::text("one"));

    // Closing one WebSocket ends its stream, not the connection.
    first
        .close(Some(CloseFrame {
            code: CloseCode::from(4000),
            reason: "bye".into(),
        }))
        .await
        .unwrap();
    assert_eq!(close_code(next_message(&mut first).await), 4000);
    assert!(
        tokio::time::timeout(Duration::from_secs(5), first.next())
            .await
            .unwrap()
            .is_none(),
        "the stream ends after the close handshake"
    );
    assert_eq!(
        server.wait_logged("echo:close", 1).await,
        ["echo:close:4000:bye"]
    );
    second.send(Message::text("still here")).await.unwrap();
    assert_eq!(next_message(&mut second).await, Message::text("still here"));
    assert_eq!(h2.get("/hello").await.0, 200);

    // A close from the server side (PHP's `$conn->close()`).
    let opened = server.logged("echo:open");
    let second_id = opened[1].trim_start_matches("echo:open:");
    assert!(server.hub.close(second_id, 4001, "kicked"));
    assert_eq!(close_code(next_message(&mut second).await), 4001);
    assert_eq!(h2.get("/hello").await.0, 200);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_room_spans_http1_and_http2_connections() {
    let server = TestServer::start(WebSocketConfig::default());
    let (mut http1, _) = connect_async(format!("ws://{}/chat", server.addr))
        .await
        .unwrap();
    let mut h2 = H2Client::connect(server.addr).await;
    let mut http2 = h2.websocket("/chat").await;
    server.wait_logged("chat:open", 2).await;
    assert_eq!(server.hub.room_count("lobby"), 2);

    http2.send(Message::text("from h2")).await.unwrap();
    assert_eq!(
        http1.next().await.unwrap().unwrap(),
        Message::text("from h2")
    );
    http1.send(Message::text("from h1")).await.unwrap();
    assert_eq!(next_message(&mut http2).await, Message::text("from h1"));
    assert_eq!(server.hub.broadcast("lobby", b"to all", false, None), 2);
    assert_eq!(next_message(&mut http2).await, Message::text("to all"));
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_client_that_stops_reading_is_evicted_without_stalling_its_connection() {
    let server = TestServer::start(WebSocketConfig {
        send_queue_len: 4,
        send_queue_bytes: 64 * 1024,
        write_timeout: Duration::from_millis(500),
        ..WebSocketConfig::default()
    });
    // Small client windows, so the unread stream fills its window quickly.
    let mut builder = http2::Builder::new(TokioExecutor::new());
    builder
        .initial_stream_window_size(64 * 1024)
        .initial_connection_window_size(4 * 1024 * 1024);
    let mut h2 = H2Client::connect_with(server.addr, builder).await;
    let _unread = h2.websocket("/chat").await;
    let mut echo = h2.websocket("/echo").await;
    server.wait_logged("chat:open", 1).await;

    let message = vec![b'x'; 16 * 1024];
    for _ in 0..64 {
        server.hub.broadcast("lobby", &message, true, None);
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    // The stalled stream's writes wait for HTTP/2 window, as writes to a full
    // HTTP/1.1 socket wait for the kernel: the write deadline ends it (1006),
    // or its bounded queue overflows first (1008). Either way memory stays
    // bounded and the stream is dropped.
    let closes = server.wait_logged("chat:close", 1).await;
    assert!(
        closes == ["chat:close:1006:"] || closes == ["chat:close:1008:slow consumer"],
        "{closes:?}"
    );
    assert_eq!(server.hub.room_count("lobby"), 0);
    echo.send(Message::text("unaffected")).await.unwrap();
    assert_eq!(next_message(&mut echo).await, Message::text("unaffected"));
    assert_eq!(h2.get("/hello").await.0, 200);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn streams_whose_handlers_fall_behind_do_not_stall_the_others() {
    const STUCK_STREAMS: usize = 3;
    let server = TestServer::start(WebSocketConfig {
        send_queue_len: 2,
        max_message_bytes: 256 * 1024,
        ..WebSocketConfig::default()
    });
    let mut h2 = H2Client::connect(server.addr).await;
    let mut echo = h2.websocket("/echo").await;

    // Each stuck stream is sent far more than one stream window (1 MiB): the
    // server stops reading it, and the client's writes on it wait for window.
    let sent = Arc::new(AtomicUsize::new(0));
    let mut writers = Vec::new();
    for _ in 0..STUCK_STREAMS {
        let mut stuck = h2.websocket("/stuck").await;
        let sent = sent.clone();
        writers.push(tokio::spawn(async move {
            let message = vec![b'y'; 200 * 1024];
            for _ in 0..20 {
                if stuck.send(Message::binary(message.clone())).await.is_err() {
                    break;
                }
                sent.fetch_add(1, Ordering::SeqCst);
            }
            stuck
        }));
    }
    server.wait_logged("stuck:message", STUCK_STREAMS).await;
    // Wait until the writers have stopped moving: the streams are full.
    loop {
        let before = sent.load(Ordering::SeqCst);
        tokio::time::sleep(Duration::from_millis(300)).await;
        if sent.load(Ordering::SeqCst) == before {
            break;
        }
    }
    assert!(
        writers.iter().all(|writer| !writer.is_finished()),
        "the stuck streams are back-pressured"
    );

    echo.send(Message::text("next door")).await.unwrap();
    let echoed = tokio::time::timeout(Duration::from_secs(2), echo.next()).await;
    assert!(
        matches!(echoed, Ok(Some(Ok(ref message))) if *message == Message::text("next door")),
        "the neighbouring stream still moves: {echoed:?}"
    );
    assert_eq!(h2.get("/hello").await.0, 200);
    for writer in writers {
        writer.abort();
    }
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn message_size_and_origin_limits_apply_to_http2_streams() {
    let server = TestServer::start(WebSocketConfig {
        max_message_bytes: 1024,
        ..WebSocketConfig::default()
    });
    let mut h2 = H2Client::connect(server.addr).await;

    let evil = [("origin", "https://evil.example")];
    assert_eq!(h2.open("/echo", "websocket", &evil).await.err(), Some(403));
    let same_origin = format!("http://{}", server.addr);
    let good = [("origin", same_origin.as_str())];
    let (mut ws, _) = h2.open("/echo", "websocket", &good).await.unwrap();

    ws.send(Message::text("z".repeat(4096))).await.unwrap();
    assert_eq!(close_code(next_message(&mut ws).await), 1009);
    assert_eq!(
        server.wait_logged("echo:close", 1).await,
        ["echo:close:1009:message too big"]
    );
    assert_eq!(h2.get("/hello").await.0, 200);
    server.stop().await;
}

#[tokio::test(flavor = "current_thread")]
async fn a_dropped_connection_runs_on_close_for_each_of_its_websockets() {
    let server = TestServer::start(WebSocketConfig::default());

    // The same drop on HTTP/1.1, for the code onClose receives.
    let (http1, _) = connect_async(format!("ws://{}/echo", server.addr))
        .await
        .unwrap();
    server.wait_logged("echo:open", 1).await;
    drop(http1);
    let http1_close = server.wait_logged("echo:close", 1).await;
    assert_eq!(http1_close, ["echo:close:1006:"]);

    let (mut client, connection) = RawH2::connect(server.addr).await;
    let first = RawH2::open(&mut client, server.addr, "/echo").await;
    let second = RawH2::open(&mut client, server.addr, "/echo").await;
    server.wait_logged("echo:open", 3).await;
    // The TCP socket closes under both streams at once, without a close
    // frame or END_STREAM on either.
    connection.abort();
    let _ = connection.await;

    let closes = server.wait_logged("echo:close", 3).await;
    assert_eq!(closes.len(), 3, "{closes:?}");
    assert!(
        closes.iter().all(|line| line == &http1_close[0]),
        "{closes:?}"
    );
    drop((first, second));
    server.stop().await;
}
