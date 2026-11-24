//! End-to-end tests for `phera serve` WebSockets using real TCP/WebSocket clients.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream as AsyncTcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

struct WsSite {
    process: Child,
    address: String,
    root: PathBuf,
    stderr_handle: Option<std::process::ChildStderr>,
}

impl WsSite {
    fn start(name: &str, workers: usize, files: &[(&str, &str)], flags: &[&str]) -> Self {
        let root = std::env::temp_dir().join(format!("phera-ws-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (path, contents) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("mkdir");
            std::fs::write(file, contents).expect("write");
        }

        let mut command = Command::new(env!("CARGO_BIN_EXE_phera"));
        command.env_clear();
        for key in ["PATH", "HOME", "TMPDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut process = command
            .args(["serve", "--listen", "127.0.0.1:0", "--workers"])
            .arg(workers.to_string())
            .current_dir(&root)
            .args(flags)
            .arg(&root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn phera serve");

        let stdout = process.stdout.take().expect("stdout");
        let stderr_handle = process.stderr.take();
        let mut banner = String::new();
        BufReader::new(stdout)
            .read_line(&mut banner)
            .expect("read banner");
        let address = banner
            .split("http://")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or_else(|| panic!("no listen address in banner: {banner:?}"))
            .to_string();

        Self {
            process,
            address,
            root,
            stderr_handle,
        }
    }

    fn ws_url(&self, path: &str) -> String {
        format!("ws://{}{path}", self.address)
    }

    fn http_post(&self, path: &str, body: &str) -> (u16, String) {
        let mut stream = TcpStream::connect(&self.address).expect("connect http");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        write!(
            stream,
            "POST {path} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.address,
            body.len()
        )
        .expect("write");
        let mut raw = String::new();
        stream.read_to_string(&mut raw).expect("read");
        let (head, resp_body) = raw.split_once("\r\n\r\n").unwrap_or((&raw, ""));
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        (status, resp_body.to_string())
    }
}

impl Drop for WsSite {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn connect_ws(
    url: &str,
    headers: &[(&str, &str)],
) -> Result<
    (
        WebSocketStream<MaybeTlsStream<AsyncTcpStream>>,
        tungstenite::handshake::client::Response,
    ),
    tungstenite::Error,
> {
    let mut request = url.into_client_request()?;
    for (name, value) in headers {
        request.headers_mut().insert(
            tungstenite::http::header::HeaderName::from_bytes(name.as_bytes())
                .expect("header name"),
            tungstenite::http::HeaderValue::from_str(value).expect("header val"),
        );
    }
    tokio_tungstenite::connect_async(request).await
}

async fn wait_for_file(path: &std::path::Path, expected_substr: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(content) = std::fs::read_to_string(path)
            && content.contains(expected_substr)
        {
            return content;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected_substr:?} in {}",
            path.display()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn snapshotted_echo_with_resolver_context_subprotocol_binary_and_close() {
    let log_file = std::env::temp_dir().join(format!("phera-ws-events-{}.log", std::process::id()));
    let _ = std::fs::remove_file(&log_file);
    let script = r#"<?php
class EchoHandler implements Phera\WebSocket\Handler
{
    public function __construct(private string $prefix) {}

    public function onOpen(Phera\WebSocket\Connection $conn): void
    {
        $ctx = $conn->context();
        $conn->setContext(['user' => $ctx['user'], 'count' => 0]);
        $conn->send($this->prefix . ':open:' . $ctx['user']);
    }

    public function onMessage(Phera\WebSocket\Connection $conn, Phera\WebSocket\Message $message): void
    {
        $ctx = $conn->context();
        $ctx['count']++;
        $conn->setContext($ctx);
        if ($message->binary) {
            $conn->send(strrev($message->data), binary: true);
        } else {
            $conn->send($this->prefix . ':' . $ctx['count'] . ':' . $message->data);
        }
    }

    public function onClose(Phera\WebSocket\Connection $conn, int $code, string $reason): void
    {
        $ctx = $conn->context();
        file_put_contents(__DIR__ . '/events.log', "close:{$code}:{$reason}:count={$ctx['count']}\n", FILE_APPEND);
    }

    public function onError(Phera\WebSocket\Connection $conn, \Throwable $error): void {}
}

Phera\WebSocket\resolveUsing(static fn (string $class): object => new $class('booted'));
Phera\snapshot();

header('Set-Cookie: ws=ok; Path=/');
Phera\WebSocket\accept(
    EchoHandler::class,
    context: ['user' => 'alice'],
    subprotocol: 'chat.v1',
);
"#;
    let site = WsSite::start("snap-echo", 1, &[("index.php", script)], &[]);
    let (mut ws, resp) = connect_ws(
        &site.ws_url("/"),
        &[("Sec-WebSocket-Protocol", "chat.v1, chat.v2")],
    )
    .await
    .expect("handshake");

    assert_eq!(resp.status().as_u16(), 101);
    assert_eq!(
        resp.headers().get("sec-websocket-protocol").unwrap(),
        "chat.v1"
    );
    assert_eq!(resp.headers().get("set-cookie").unwrap(), "ws=ok; Path=/");

    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Message::Text("booted:open:alice".into())
    );
    ws.send(Message::Text("hi".into())).await.unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Message::Text("booted:1:hi".into())
    );
    ws.send(Message::Binary(vec![1, 2, 3].into()))
        .await
        .unwrap();
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Message::Binary(vec![3, 2, 1].into())
    );
    ws.close(None).await.unwrap();

    let events = wait_for_file(&site.root.join("events.log"), "close:1000").await;
    assert!(events.contains("count=2"), "{events}");
}

#[tokio::test(flavor = "current_thread")]
async fn classic_unsnapshotted_script_uses_is_event_and_enforces_order() {
    let script = r#"<?php
class OrderedEcho implements Phera\WebSocket\Handler
{
    public function onOpen(Phera\WebSocket\Connection $conn): void {}
    public function onMessage(Phera\WebSocket\Connection $conn, Phera\WebSocket\Message $message): void
    {
        // Calling accept() inside an event fork must fail cleanly if attempted only for negative case.
        if ($message->data === 'try-accept') {
            Phera\WebSocket\accept(self::class);
        }
        $conn->send('seq:' . $message->data);
    }
    public function onClose(Phera\WebSocket\Connection $conn, int $code, string $reason): void {}
    public function onError(Phera\WebSocket\Connection $conn, \Throwable $error): void
    {
        file_put_contents(__DIR__ . '/err.log', get_class($error) . ':' . $error->getMessage() . "\n", FILE_APPEND);
    }
}

if (!Phera\WebSocket\isEvent()) {
    Phera\WebSocket\accept(OrderedEcho::class);
}
"#;
    let site = WsSite::start("classic-order", 1, &[("ws.php", script)], &[]);
    let (mut ws, _) = connect_ws(&site.ws_url("/ws.php"), &[])
        .await
        .expect("connect");

    for i in 1..=8 {
        ws.send(Message::Text(i.to_string().into())).await.unwrap();
    }
    for i in 1..=8 {
        assert_eq!(
            ws.next().await.unwrap().unwrap(),
            Message::Text(format!("seq:{i}").into())
        );
    }

    // Calling accept() inside an event triggers LogicException -> onError -> 1011 close.
    ws.send(Message::Text("try-accept".into())).await.unwrap();
    let frame = ws.next().await.unwrap().unwrap();
    assert_eq!(
        frame,
        Message::Close(Some(tungstenite::protocol::CloseFrame {
            code: CloseCode::Error,
            reason: "internal error".into(),
        }))
    );
    let err_log = wait_for_file(&site.root.join("err.log"), "LogicException").await;
    assert!(err_log.contains("accept()"), "{err_log}");
}

#[tokio::test(flavor = "current_thread")]
async fn rooms_broadcast_http_push_across_workers() {
    let script = r#"<?php
class RoomHandler implements Phera\WebSocket\Handler
{
    public function onOpen(Phera\WebSocket\Connection $conn): void
    {
        $conn->send('id:' . $conn->id() . ':rooms=' . implode(',', $conn->rooms()));
    }
    public function onMessage(Phera\WebSocket\Connection $conn, Phera\WebSocket\Message $message): void
    {
        if (str_starts_with($message->data, 'join:')) {
            $room = substr($message->data, 5);
            $conn->join($room);
            $conn->send('joined:' . implode(',', $conn->rooms()));
            return;
        }
        $conn->broadcast('chat', $message->data);
    }
    public function onClose(Phera\WebSocket\Connection $conn, int $code, string $reason): void {}
    public function onError(Phera\WebSocket\Connection $conn, \Throwable $error): void {}
}

Phera\snapshot();

if ($_SERVER['REQUEST_METHOD'] === 'POST') {
    $msg = file_get_contents('php://input');
    $count = Phera\WebSocket\broadcast('chat', "http:$msg");
    echo "sent=$count,room=" . Phera\WebSocket\roomCount('chat') . ',ids=' . count(Phera\WebSocket\connections('chat'));
    return;
}

Phera\WebSocket\accept(RoomHandler::class, rooms: ['chat']);
"#;
    let site = WsSite::start("rooms-multi", 2, &[("index.php", script)], &[]);
    let (mut ws1, _) = connect_ws(&site.ws_url("/"), &[]).await.expect("ws1");
    let (mut ws2, _) = connect_ws(&site.ws_url("/"), &[]).await.expect("ws2");

    assert!(
        ws1.next()
            .await
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap()
            .ends_with(":rooms=chat")
    );
    assert!(
        ws2.next()
            .await
            .unwrap()
            .unwrap()
            .to_text()
            .unwrap()
            .ends_with(":rooms=chat")
    );

    ws1.send(Message::Text("hello all".into())).await.unwrap();
    assert_eq!(
        ws2.next().await.unwrap().unwrap(),
        Message::Text("hello all".into())
    );

    let (status, body) = site.http_post("/", "banner");
    assert_eq!(status, 200);
    assert_eq!(body, "sent=2,room=2,ids=2");
    assert_eq!(
        ws1.next().await.unwrap().unwrap(),
        Message::Text("http:banner".into())
    );
    assert_eq!(
        ws2.next().await.unwrap().unwrap(),
        Message::Text("http:banner".into())
    );

    ws1.send(Message::Text("join:vip".into())).await.unwrap();
    assert_eq!(
        ws1.next().await.unwrap().unwrap(),
        Message::Text("joined:chat,vip".into())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn context_validation_auth_origin_and_trusted_proxy() {
    let script = r#"<?php
class AuthHandler implements Phera\WebSocket\Handler
{
    public function onOpen(Phera\WebSocket\Connection $conn): void { $conn->send('ok'); }
    public function onMessage(Phera\WebSocket\Connection $conn, Phera\WebSocket\Message $message): void
    {
        try {
            $conn->setContext(['bad' => new stdClass()]);
        } catch (\TypeError $e) {
            $conn->send('caught-type:' . $e->getMessage());
            return;
        }
        $conn->send('unexpected');
    }
    public function onClose(Phera\WebSocket\Connection $conn, int $code, string $reason): void {}
    public function onError(Phera\WebSocket\Connection $conn, \Throwable $error): void {}
}

if (($_SERVER['REQUEST_URI'] ?? '') === '/auth-fail') {
    http_response_code(401);
    echo 'denied';
    return;
}
Phera\WebSocket\accept(AuthHandler::class);
"#;
    let site = WsSite::start(
        "auth-origin",
        1,
        &[("index.php", script)],
        &["--trusted-proxies", "127.0.0.1"],
    );

    // 401 before 101 handshake.
    let err = connect_ws(&site.ws_url("/auth-fail"), &[])
        .await
        .unwrap_err();
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(resp.status().as_u16(), 401),
        other => panic!("expected HTTP 401, got {other:?}"),
    }

    // Wrong Origin rejected with 403.
    let err = connect_ws(&site.ws_url("/"), &[("Origin", "https://evil.example")])
        .await
        .unwrap_err();
    match err {
        tungstenite::Error::Http(resp) => assert_eq!(resp.status().as_u16(), 403),
        other => panic!("expected HTTP 403, got {other:?}"),
    }

    // Trusted proxy X-Forwarded-* accepted.
    let (mut ws, resp) = connect_ws(
        &site.ws_url("/"),
        &[
            ("X-Forwarded-Proto", "https"),
            ("X-Forwarded-Host", "ws.example.test"),
            ("Origin", "https://ws.example.test"),
        ],
    )
    .await
    .expect("proxy origin allowed");
    assert_eq!(resp.status().as_u16(), 101);
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Message::Text("ok".into())
    );

    ws.send(Message::Text("test".into())).await.unwrap();
    let msg = ws.next().await.unwrap().unwrap();
    assert!(msg.to_text().unwrap().starts_with("caught-type:"));
}

#[tokio::test(flavor = "current_thread")]
async fn error_handler_and_dev_missing_class_and_toml_limits() {
    let script = r#"<?php
require_once __DIR__ . '/handler.php';

if (!Phera\WebSocket\isEvent()) {
    Phera\WebSocket\accept(DynamicHandler::class);
}
"#;
    let handler_v1 = r#"<?php
class DynamicHandler implements Phera\WebSocket\Handler
{
    public function onOpen(Phera\WebSocket\Connection $conn): void { $conn->send('ready'); }
    public function onMessage(Phera\WebSocket\Connection $conn, Phera\WebSocket\Message $message): void
    {
        if ($message->data === 'throw-twice') {
            throw new \RuntimeException('boom');
        }
        $conn->send($message->data);
    }
    public function onClose(Phera\WebSocket\Connection $conn, int $code, string $reason): void
    {
        file_put_contents(__DIR__ . '/trace.log', "onClose:$code\n", FILE_APPEND);
        throw new \RuntimeException('close-fail');
    }
    public function onError(Phera\WebSocket\Connection $conn, \Throwable $error): void
    {
        file_put_contents(__DIR__ . '/trace.log', "onError:{$error->getMessage()}\n", FILE_APPEND);
        throw new \RuntimeException('error-handler-threw');
    }
}
"#;
    let toml = r#"[server.websocket]
max_message_size = 64
allowed_origins = ["*"]
"#;
    let mut site = WsSite::start(
        "errors-dev",
        1,
        &[
            ("index.php", script),
            ("handler.php", handler_v1),
            ("phera.toml", toml),
        ],
        &["--dev"],
    );

    // 1. Oversized message triggers 1009 Too Big from phera.toml limit.
    let (mut ws_big, _) = connect_ws(&site.ws_url("/"), &[("Origin", "https://any.origin")])
        .await
        .expect("connect");
    assert_eq!(
        ws_big.next().await.unwrap().unwrap(),
        Message::Text("ready".into())
    );
    ws_big
        .send(Message::Text("x".repeat(128).into()))
        .await
        .unwrap();
    let frame = ws_big.next().await.unwrap().unwrap();
    match frame {
        Message::Close(Some(cf)) => assert_eq!(cf.code, CloseCode::Size),
        other => panic!("expected 1009 Size close, got {other:?}"),
    }

    // 2. Throw in onMessage -> onError runs once and throws -> no recursion -> 1011, then onClose throws -> logged, no onError recursion.
    let (mut ws_err, _) = connect_ws(&site.ws_url("/"), &[]).await.expect("connect");
    assert_eq!(
        ws_err.next().await.unwrap().unwrap(),
        Message::Text("ready".into())
    );
    ws_err
        .send(Message::Text("throw-twice".into()))
        .await
        .unwrap();
    let frame = ws_err.next().await.unwrap().unwrap();
    match frame {
        Message::Close(Some(cf)) => assert_eq!(cf.code, CloseCode::Error),
        other => panic!("expected 1011 Error close, got {other:?}"),
    }
    let trace = wait_for_file(&site.root.join("trace.log"), "onClose:").await;
    assert_eq!(trace.matches("onError:").count(), 1, "{trace}");

    // 3. Delete handler class under --dev between onOpen and onMessage -> 1011 without calling onError.
    let (mut ws_dev, _) = connect_ws(&site.ws_url("/"), &[]).await.expect("connect");
    assert_eq!(
        ws_dev.next().await.unwrap().unwrap(),
        Message::Text("ready".into())
    );
    std::fs::write(site.root.join("handler.php"), "<?php // handler removed\n").unwrap();
    ws_dev.send(Message::Text("hello".into())).await.unwrap();
    let frame = ws_dev.next().await.unwrap().unwrap();
    match frame {
        Message::Close(Some(cf)) => assert_eq!(cf.code, CloseCode::Error),
        other => panic!("expected 1011 Error close, got {other:?}"),
    }
    let trace_after = std::fs::read_to_string(site.root.join("trace.log")).unwrap();
    assert_eq!(trace_after.matches("onError:").count(), 1, "{trace_after}");

    // Verify stderr captured logs from error-handler-threw and close-fail cleanly.
    let _ = site.process.kill();
    let _ = site.process.wait();
    let mut stderr = String::new();
    if let Some(mut pipe) = site.stderr_handle.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    assert!(
        stderr.contains("boom") && stderr.contains("error-handler-threw"),
        "{stderr}"
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn sigterm_shutdown_sends_1001_and_runs_on_close() {
    let script = r#"<?php
class ShutdownHandler implements Phera\WebSocket\Handler
{
    public function onOpen(Phera\WebSocket\Connection $conn): void { $conn->send('hi'); }
    public function onMessage(Phera\WebSocket\Connection $conn, Phera\WebSocket\Message $message): void {}
    public function onClose(Phera\WebSocket\Connection $conn, int $code, string $reason): void
    {
        file_put_contents(__DIR__ . '/shutdown.log', "closed:$code:$reason\n");
    }
    public function onError(Phera\WebSocket\Connection $conn, \Throwable $error): void {}
}

Phera\snapshot();
Phera\WebSocket\accept(ShutdownHandler::class);
"#;
    let mut site = WsSite::start("sigterm", 1, &[("index.php", script)], &[]);
    let (mut ws, _) = connect_ws(&site.ws_url("/"), &[]).await.expect("connect");
    assert_eq!(
        ws.next().await.unwrap().unwrap(),
        Message::Text("hi".into())
    );

    let pid = site.process.id().to_string();
    let status = Command::new("kill")
        .args(["-TERM", &pid])
        .status()
        .expect("send SIGTERM");
    assert!(status.success());

    let frame = ws.next().await.unwrap().unwrap();
    match frame {
        Message::Close(Some(cf)) => assert_eq!(cf.code, CloseCode::Away),
        other => panic!("expected 1001 Away close frame, got {other:?}"),
    }
    let log = wait_for_file(&site.root.join("shutdown.log"), "closed:1001").await;
    assert!(log.contains("closed:1001"), "{log}");
    let exit = site.process.wait().expect("wait exited");
    assert!(exit.success(), "clean shutdown exit: {exit:?}");
}
