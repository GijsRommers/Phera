//! Integration tests for `phera-server` WebSockets over HTTP/1.1 upgrade:
//! framing, fragmentation, ping/pong, origin checks, per-connection ordering,
//! cross-connection concurrency, rooms, backpressure eviction, and shutdown.

mod common;

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use phera_server::{
    Handler, HandlerFactory, Request, Response, Server, ServerConfig, WebSocketConfig,
    WebSocketEvent, WebSocketEventKind, WebSocketHub, WebSocketOutcome, WebSocketUpgrade,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::frame::Frame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::{Data, OpCode};

struct WsTestFactory {
    hub: Arc<WebSocketHub>,
    log: Arc<Mutex<Vec<String>>>,
    active_events: Arc<AtomicUsize>,
}

impl HandlerFactory for WsTestFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(WsTestHandler {
            hub: self.hub.clone(),
            log: self.log.clone(),
            active_events: self.active_events.clone(),
        })
    }
}

struct WsTestHandler {
    hub: Arc<WebSocketHub>,
    log: Arc<Mutex<Vec<String>>>,
    active_events: Arc<AtomicUsize>,
}

impl Handler for WsTestHandler {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        let hub = self.hub.clone();
        Box::pin(async move {
            match request.path.as_str() {
                "/deny" => Response::text(401, "unauthorized\n"),
                "/push" => {
                    let count = hub.broadcast("lobby", &request.body, false, None);
                    Response::text(200, &count.to_string())
                }
                "/flood" => {
                    for idx in 0..20 {
                        let msg = format!("flood-{idx:02}-{}", "x".repeat(40));
                        hub.broadcast("lobby", msg.as_bytes(), false, None);
                    }
                    Response::text(200, "ok\n")
                }
                "/echo" => Response::new(101)
                    .with_header("set-cookie", "ws_session=ok")
                    .with_websocket_upgrade(WebSocketUpgrade {
                        entry: "echo".to_string(),
                        handler_class: "Echo".to_string(),
                        subprotocol: Some("chat.v1".to_string()),
                        context: Vec::new(),
                        rooms: Vec::new(),
                    }),
                "/chat" => Response::new(101).with_websocket_upgrade(WebSocketUpgrade {
                    entry: "chat".to_string(),
                    handler_class: "Chat".to_string(),
                    subprotocol: None,
                    context: Vec::new(),
                    rooms: vec!["lobby".to_string()],
                }),
                "/ordered" => Response::new(101).with_websocket_upgrade(WebSocketUpgrade {
                    entry: "ordered".to_string(),
                    handler_class: "Ordered".to_string(),
                    subprotocol: None,
                    context: Vec::new(),
                    rooms: Vec::new(),
                }),
                "/slow_open" => Response::new(101).with_websocket_upgrade(WebSocketUpgrade {
                    entry: "slow_open".to_string(),
                    handler_class: "SlowOpen".to_string(),
                    subprotocol: None,
                    context: Vec::new(),
                    rooms: Vec::new(),
                }),
                _ => Response::text(404, "not found\n"),
            }
        })
    }

    fn handle_websocket(
        &self,
        event: WebSocketEvent,
    ) -> Pin<Box<dyn Future<Output = WebSocketOutcome> + '_>> {
        let hub = self.hub.clone();
        let log = self.log.clone();
        let active_events = self.active_events.clone();
        Box::pin(async move {
            match (&*event.entry, event.kind) {
                ("slow_open", WebSocketEventKind::Open) => {
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    log.lock()
                        .unwrap()
                        .push(format!("slow_open:open:{}", event.connection_id));
                }
                (entry, WebSocketEventKind::Open) => {
                    log.lock()
                        .unwrap()
                        .push(format!("{entry}:open:{}", event.connection_id));
                }
                ("echo", WebSocketEventKind::Message { data, binary }) => {
                    if !binary && data == b"fail" {
                        return WebSocketOutcome { close_1011: true };
                    }
                    hub.send(&event.connection_id, &data, binary);
                }
                ("chat", WebSocketEventKind::Message { data, binary }) => {
                    hub.broadcast("lobby", &data, binary, Some(&event.connection_id));
                }
                ("ordered", WebSocketEventKind::Message { data, binary }) => {
                    let concurrent = active_events.fetch_add(1, Ordering::SeqCst);
                    log.lock().unwrap().push(format!(
                        "ordered:start:{}:{}:{concurrent}",
                        event.connection_id,
                        String::from_utf8_lossy(&data)
                    ));
                    if data == b"slow" {
                        tokio::time::sleep(Duration::from_millis(120)).await;
                    }
                    active_events.fetch_sub(1, Ordering::SeqCst);
                    hub.send(&event.connection_id, &data, binary);
                }
                (entry, WebSocketEventKind::Close { code, reason }) => {
                    log.lock()
                        .unwrap()
                        .push(format!("{entry}:close:{code}:{reason}"));
                }
                _ => {}
            }
            WebSocketOutcome::default()
        })
    }
}

fn spawn_test_server(
    config: ServerConfig,
) -> (
    phera_server::ServerHandle,
    std::net::SocketAddr,
    Arc<WebSocketHub>,
    Arc<Mutex<Vec<String>>>,
) {
    let hub = Arc::new(WebSocketHub::new(
        config.websocket.send_queue_len,
        config.websocket.send_queue_bytes,
    ));
    let log = Arc::new(Mutex::new(Vec::new()));
    let factory = WsTestFactory {
        hub: hub.clone(),
        log: log.clone(),
        active_events: Arc::new(AtomicUsize::new(0)),
    };
    let server = Server::bind_with_hub(config, factory, hub.clone()).unwrap();
    let addr = server.local_addr();
    (server.spawn(), addr, hub, log)
}

#[tokio::test(flavor = "current_thread")]
async fn echo_text_binary_fragmented_ping_and_auth_and_1011() {
    let config = ServerConfig {
        workers: 1,
        ..common::test_config()
    };
    let (server, addr, _hub, log) = spawn_test_server(config);

    // Un-upgraded HTTP request to an upgrade handler gets 426.
    let resp = common::Client::connect(addr).request("GET /echo HTTP/1.1", &[], b"");
    assert_eq!(resp.status, 426);

    // Denied upgrade returns PHP's 401 without upgrading.
    let mut deny_req = format!("ws://{addr}/deny").into_client_request().unwrap();
    deny_req
        .headers_mut()
        .insert("host", HeaderValue::from_str(&addr.to_string()).unwrap());
    let err = connect_async(deny_req).await.unwrap_err();
    match err {
        tokio_tungstenite::tungstenite::Error::Http(resp) => {
            assert_eq!(resp.status().as_u16(), 401);
        }
        other => panic!("expected Http 401, got {other:?}"),
    }

    // Successful upgrade preserves custom headers and subprotocol.
    let mut req = format!("ws://{addr}/echo").into_client_request().unwrap();
    req.headers_mut().insert(
        "sec-websocket-protocol",
        HeaderValue::from_static("chat.v1"),
    );
    let (mut ws, resp) = connect_async(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 101);
    assert_eq!(resp.headers().get("set-cookie").unwrap(), "ws_session=ok");
    assert_eq!(
        resp.headers().get("sec-websocket-protocol").unwrap(),
        "chat.v1"
    );

    // Text and binary echo.
    ws.send(Message::text("hello world")).await.unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Message::text("hello world")
    );
    ws.send(Message::binary(vec![1, 2, 3, 4])).await.unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Message::binary(vec![1, 2, 3, 4])
    );

    // Fragmented message reassembled into one text frame.
    ws.send(Message::Frame(Frame::message(
        b"part1-".to_vec(),
        OpCode::Data(Data::Text),
        false,
    )))
    .await
    .unwrap();
    ws.send(Message::Frame(Frame::message(
        b"part2".to_vec(),
        OpCode::Data(Data::Continue),
        true,
    )))
    .await
    .unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Message::text("part1-part2")
    );

    // Ping answered with Pong in Rust without entering handle_websocket.
    ws.send(Message::Ping(b"hb".to_vec().into())).await.unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Message::Pong(b"hb".to_vec().into())
    );

    // Handler failure triggers close code 1011.
    ws.send(Message::text("fail")).await.unwrap();
    let closed = ws.next().await.unwrap().unwrap();
    match closed {
        Message::Close(Some(frame)) => assert_eq!(u16::from(frame.code), 1011),
        other => panic!("expected close 1011, got {other:?}"),
    }

    assert!(
        log.lock()
            .unwrap()
            .iter()
            .any(|entry| entry.starts_with("echo:open:"))
    );
    server.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn origin_validation_and_size_limit_reject_disallowed_clients() {
    let mut config = common::test_config();
    config.workers = 1;
    config.websocket.max_message_bytes = 64;
    let (server, addr, _hub, _log) = spawn_test_server(config);

    // Cross-origin browser request fails 403 by default.
    let mut bad_origin = format!("ws://{addr}/echo").into_client_request().unwrap();
    bad_origin
        .headers_mut()
        .insert("origin", HeaderValue::from_static("https://evil.example"));
    let err = connect_async(bad_origin).await.unwrap_err();
    match err {
        tokio_tungstenite::tungstenite::Error::Http(resp) => {
            assert_eq!(resp.status().as_u16(), 403);
        }
        other => panic!("expected Http 403, got {other:?}"),
    }

    // Same-origin succeeds and rejects > 64 byte frames with 1009.
    let mut same_origin = format!("ws://{addr}/echo").into_client_request().unwrap();
    same_origin.headers_mut().insert(
        "origin",
        HeaderValue::from_str(&format!("http://{addr}")).unwrap(),
    );
    let (mut ws, _) = connect_async(same_origin).await.unwrap();
    ws.send(Message::text("x".repeat(200))).await.unwrap();
    match ws.next().await {
        Some(Ok(Message::Close(Some(frame)))) => assert_eq!(u16::from(frame.code), 1009),
        Some(Err(_)) | None => {}
        other => panic!("expected 1009 or error close, got {other:?}"),
    }

    server.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn per_connection_events_are_strictly_ordered_while_connections_run_concurrently() {
    let config = ServerConfig {
        workers: 1,
        ..common::test_config()
    };
    let (server, addr, _hub, _log) = spawn_test_server(config);

    let (mut ws_a, _) = connect_async(format!("ws://{addr}/ordered")).await.unwrap();
    let (mut ws_b, _) = connect_async(format!("ws://{addr}/ordered")).await.unwrap();

    // Send slow then fast on connection A; fast must still come after slow.
    ws_a.send(Message::text("slow")).await.unwrap();
    ws_a.send(Message::text("after-slow")).await.unwrap();

    // While A is sleeping 120ms, connection B sends and must receive immediately.
    tokio::time::sleep(Duration::from_millis(20)).await;
    ws_b.send(Message::text("fast-b")).await.unwrap();
    let b_reply = tokio::time::timeout(Duration::from_millis(80), ws_b.next())
        .await
        .expect("connection B should not wait for connection A")
        .unwrap()
        .unwrap();
    assert_eq!(b_reply, Message::text("fast-b"));

    let a_first = ws_a.next().await.unwrap().unwrap();
    let a_second = ws_a.next().await.unwrap().unwrap();
    assert_eq!(a_first, Message::text("slow"));
    assert_eq!(a_second, Message::text("after-slow"));

    server.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn chat_room_broadcast_http_push_and_slow_consumer_eviction() {
    let config = ServerConfig {
        workers: 2,
        websocket: WebSocketConfig {
            send_queue_len: 4,
            send_queue_bytes: 256,
            ..WebSocketConfig::default()
        },
        ..common::test_config()
    };
    let (server, addr, hub, _log) = spawn_test_server(config);

    let (mut ws1, _) = connect_async(format!("ws://{addr}/chat")).await.unwrap();
    let (mut ws2, _) = connect_async(format!("ws://{addr}/chat")).await.unwrap();

    // Wait for both Open events to complete room registration.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(hub.room_count("lobby"), 2);

    // ws1 broadcasts to room; ws2 receives, ws1 does not (excludeSelf).
    ws1.send(Message::text("hi room")).await.unwrap();
    assert_eq!(ws2.next().await.unwrap().unwrap(), Message::text("hi room"));

    // Ordinary HTTP POST pushes to both connections in lobby.
    let mut http = TcpStream::connect(addr).await.unwrap();
    http.write_all(
        format!("POST /push HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: 9\r\n\r\nfrom-http").as_bytes(),
    )
    .await
    .unwrap();
    let mut resp = String::new();
    http.read_to_string(&mut resp).await.unwrap();
    assert!(resp.ends_with('2'), "{resp}");
    assert_eq!(
        ws1.next().await.unwrap().unwrap(),
        Message::text("from-http")
    );
    assert_eq!(
        ws2.next().await.unwrap().unwrap(),
        Message::text("from-http")
    );
    drop(ws2);
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Keep a slow consumer on worker 0 / room lobby while bursting 20 frames into
    // its bounded queue (send_queue_len = 4 / send_queue_bytes = 256) so it is evicted.
    let (mut ws_slow, _) = connect_async(format!("ws://{addr}/chat")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(hub.room_count("lobby"), 2);
    let mut flood_req = TcpStream::connect(addr).await.unwrap();
    flood_req
        .write_all(format!("POST /flood HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n").as_bytes())
        .await
        .unwrap();
    let mut flood_resp = String::new();
    flood_req.read_to_string(&mut flood_resp).await.unwrap();
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(hub.room_count("lobby"), 0);
    let _ = ws_slow.close(None).await;
    // Reconnect one clean client so shutdown test verifies Close(1001).
    let (mut ws1, _) = connect_async(format!("ws://{addr}/chat")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(30)).await;

    // Graceful shutdown sends 1001 Going Away to ws1.
    server.shutdown();
    let closed = ws1.next().await.unwrap().unwrap();
    match closed {
        Message::Close(Some(frame)) => assert_eq!(u16::from(frame.code), 1001),
        other => panic!("expected close 1001 on shutdown, got {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn on_close_still_runs_when_event_queue_is_full() {
    let config = ServerConfig {
        workers: 1,
        websocket: WebSocketConfig {
            send_queue_len: 1,
            ..WebSocketConfig::default()
        },
        ..common::test_config()
    };
    let (server, addr, _hub, log) = spawn_test_server(config);

    let (mut ws, _) = connect_async(format!("ws://{addr}/slow_open"))
        .await
        .unwrap();
    // onOpen sleeps 80ms; send one frame to fill the 1-slot event queue and close right away.
    ws.send(Message::text("queued-msg")).await.unwrap();
    ws.close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
        code: tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode::Normal,
        reason: "done".into(),
    }))
    .await
    .unwrap();
    let _ = ws.next().await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        if log
            .lock()
            .unwrap()
            .iter()
            .any(|entry| entry == "slow_open:close:1000:done")
        {
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "onClose missing from log: {:?}",
            *log.lock().unwrap()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    server.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn close_still_disconnects_when_outbound_queue_is_full() {
    let config = ServerConfig {
        workers: 1,
        websocket: WebSocketConfig {
            send_queue_len: 2,
            ..WebSocketConfig::default()
        },
        ..common::test_config()
    };
    let (server, addr, hub, log) = spawn_test_server(config);

    let (mut ws, _) = connect_async(format!("ws://{addr}/echo")).await.unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let conn_id = loop {
        if let Some(id) = log
            .lock()
            .unwrap()
            .iter()
            .find_map(|entry| entry.strip_prefix("echo:open:"))
            .map(str::to_owned)
        {
            break id;
        }
        assert!(tokio::time::Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(10)).await;
    };

    // Fill the 2-slot outbound queue before close() so close() hits TrySendError::Full.
    assert!(hub.send(&conn_id, b"msg-1", false));
    assert!(hub.send(&conn_id, b"msg-2", false));
    assert!(hub.close(&conn_id, 4000, "full"));

    let frame = tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Close(Some(frame)))) => return Some(frame),
                Some(Ok(_)) => continue,
                _ => return None,
            }
        }
    })
    .await
    .expect("socket should close promptly even with full queue")
    .expect("expected Close frame");
    assert_eq!(u16::from(frame.code), 4000);
    assert_eq!(frame.reason.as_str(), "full");
    server.shutdown();
}

async fn raw_ws_connect_and_send(addr: std::net::SocketAddr, frame_bytes: &[u8]) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "GET /echo HTTP/1.1\r\nHost: {addr}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
    );
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream.read(&mut tmp).await.unwrap();
        assert!(n > 0, "unexpected EOF waiting for 101");
        buf.extend_from_slice(&tmp[..n]);
        if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = std::str::from_utf8(&buf[..end]).unwrap();
            assert!(head.starts_with("HTTP/1.1 101 "), "{head}");
            buf.drain(..end + 4);
            break;
        }
    }
    stream.write_all(frame_bytes).await.unwrap();

    while buf.len() < 4 {
        let n = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut tmp))
            .await
            .expect("timeout waiting for server close frame")
            .unwrap();
        assert!(n > 0, "unexpected EOF waiting for server close frame");
        buf.extend_from_slice(&tmp[..n]);
    }
    assert_eq!(
        buf[0] & 0x0F,
        0x08,
        "expected Close opcode frame: {buf:02x?}"
    );
    let len = (buf[1] & 0x7F) as usize;
    while buf.len() < 2 + len {
        let n = stream.read(&mut tmp).await.unwrap();
        assert!(n > 0);
        buf.extend_from_slice(&tmp[..n]);
    }
    let code = u16::from_be_bytes([buf[2], buf[3]]);
    let reason = String::from_utf8_lossy(&buf[4..2 + len]).into_owned();
    (code, reason)
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_utf8_closes_with_1007_and_protocol_error_closes_with_1002() {
    let config = ServerConfig {
        workers: 1,
        ..common::test_config()
    };
    let (server, addr, _hub, _log) = spawn_test_server(config);

    // Invalid UTF-8 inside a masked client text frame -> 1007.
    let invalid_utf8_frame = [0x81, 0x82, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFE];
    let (code_utf8, _) = raw_ws_connect_and_send(addr, &invalid_utf8_frame).await;
    assert_eq!(code_utf8, 1007);

    // Unmasked client frame violates RFC 6455 -> 1002.
    let unmasked_frame = [0x81, 0x02, b'h', b'i'];
    let (code_proto, _) = raw_ws_connect_and_send(addr, &unmasked_frame).await;
    assert_eq!(code_proto, 1002);

    server.shutdown();
}
