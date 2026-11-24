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
            match (event.entry.as_str(), event.kind) {
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
