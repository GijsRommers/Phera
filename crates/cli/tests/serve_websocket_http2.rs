//! End-to-end WebSockets over HTTP/2 (RFC 8441 extended CONNECT) against the
//! `phera serve` binary: the PHP side is the one written for HTTP/1.1.

use std::io::{BufRead, BufReader};
use std::net::Shutdown;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper::client::conn::http2;
use hyper::ext::Protocol;
use hyper::{Method, Request};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::Role;

const SCRIPT: &str = r#"<?php
class Chat implements Phera\WebSocket\Handler
{
    public function onOpen(Phera\WebSocket\Connection $conn): void
    {
        $seen = $conn->context();
        $conn->send("open:{$seen['method']}:{$seen['protocol']}:{$seen['upgrade']}");
    }
    public function onMessage(Phera\WebSocket\Connection $conn, Phera\WebSocket\Message $message): void
    {
        $conn->broadcast('chat', $message->data);
    }
    public function onClose(Phera\WebSocket\Connection $conn, int $code, string $reason): void
    {
        file_put_contents(__DIR__ . '/close.log', "close:$code\n", FILE_APPEND);
    }
    public function onError(Phera\WebSocket\Connection $conn, \Throwable $error): void {}
}

Phera\snapshot();

if ($_SERVER['REQUEST_METHOD'] === 'POST') {
    echo 'sent=' . Phera\WebSocket\broadcast('chat', file_get_contents('php://input'));
    return;
}
Phera\WebSocket\accept(Chat::class, rooms: ['chat'], context: [
    'method' => $_SERVER['REQUEST_METHOD'],
    'protocol' => $_SERVER['SERVER_PROTOCOL'],
    'upgrade' => $_SERVER['HTTP_UPGRADE'] ?? '',
]);
"#;

struct Site {
    process: Child,
    address: String,
    root: PathBuf,
}

impl Site {
    fn start(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("phera-ws-h2-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create dir");
        std::fs::write(root.join("index.php"), SCRIPT).expect("write script");
        let mut command = Command::new(env!("CARGO_BIN_EXE_phera"));
        command.env_clear();
        for key in ["PATH", "HOME", "TMPDIR"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let mut process = command
            .args(["serve", "--listen", "127.0.0.1:0", "--workers", "1"])
            .current_dir(&root)
            .arg(&root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start phera serve");
        let mut banner = String::new();
        BufReader::new(process.stdout.take().expect("stdout"))
            .read_line(&mut banner)
            .expect("read banner");
        let address = banner
            .split("http://")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or_else(|| panic!("no address in banner: {banner:?}"))
            .to_string();
        Self {
            process,
            address,
            root,
        }
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

type H2WebSocket = WebSocketStream<TokioIo<hyper::upgrade::Upgraded>>;

/// An `h2c` connection whose socket the test can cut (`socket.shutdown`).
struct H2Connection {
    address: String,
    sender: http2::SendRequest<Empty<Bytes>>,
    socket: std::net::TcpStream,
}

impl H2Connection {
    async fn connect(address: &str) -> Self {
        let socket = std::net::TcpStream::connect(address).expect("connect");
        socket.set_nonblocking(true).expect("non-blocking");
        let tcp = TcpStream::from_std(socket.try_clone().expect("clone")).expect("tokio socket");
        let (sender, connection) = http2::Builder::new(TokioExecutor::new())
            .handshake(TokioIo::new(tcp))
            .await
            .expect("h2 handshake");
        tokio::spawn(async move {
            let _ = connection.await;
        });
        let mut connection = Self {
            address: address.to_string(),
            sender,
            socket,
        };
        // `:protocol` may be sent once the server's SETTINGS arrived; the
        // answer to a first request guarantees that. A plain GET that calls
        // `accept()` is not an upgrade, so it is answered 426.
        assert_eq!(connection.request(Method::GET).await.0, 426);
        connection
    }

    async fn request(&mut self, method: Method) -> (u16, String) {
        let request = Request::builder()
            .method(method)
            .uri(format!("http://{}/", self.address))
            .body(Empty::new())
            .expect("request");
        self.sender.ready().await.expect("ready");
        let response = self.sender.send_request(request).await.expect("response");
        let status = response.status().as_u16();
        let body = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    /// Open a WebSocket stream; `Err` carries the status of a refusal.
    async fn websocket(&mut self, origin: Option<&str>) -> Result<H2WebSocket, u16> {
        let mut builder = Request::builder()
            .method(Method::CONNECT)
            .uri(format!("http://{}/", self.address))
            .header("sec-websocket-version", "13");
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        let mut request = builder.body(Empty::new()).expect("request");
        request
            .extensions_mut()
            .insert(Protocol::from_static("websocket"));
        self.sender.ready().await.expect("ready");
        let response = self.sender.send_request(request).await.expect("response");
        if response.status() != 200 {
            return Err(response.status().as_u16());
        }
        let upgraded = hyper::upgrade::on(response).await.expect("upgraded stream");
        Ok(WebSocketStream::from_raw_socket(TokioIo::new(upgraded), Role::Client, None).await)
    }
}

async fn next_text<S>(ws: &mut S) -> String
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let message = tokio::time::timeout(Duration::from_secs(10), ws.next())
        .await
        .expect("a message within ten seconds")
        .expect("open stream")
        .expect("valid frame");
    message.into_text().expect("text").to_string()
}

async fn wait_for_lines(path: &Path, lines: usize) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        if text.lines().count() >= lines || Instant::now() > deadline {
            return text;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn php_sees_one_upgrade_request_and_rooms_span_both_transports() {
    let site = Site::start("rooms");
    let (mut http1, _) = tokio_tungstenite::connect_async(format!("ws://{}/", site.address))
        .await
        .expect("HTTP/1.1 WebSocket");
    assert_eq!(next_text(&mut http1).await, "open:GET:HTTP/1.1:websocket");

    let mut h2 = H2Connection::connect(&site.address).await;
    let mut http2 = h2.websocket(None).await.expect("HTTP/2 WebSocket");
    assert_eq!(next_text(&mut http2).await, "open:GET:HTTP/2.0:websocket");

    http2.send(Message::text("from h2")).await.unwrap();
    assert_eq!(next_text(&mut http1).await, "from h2");
    http1.send(Message::text("from h1")).await.unwrap();
    assert_eq!(next_text(&mut http2).await, "from h1");

    // An ordinary request on the same HTTP/2 connection (an empty POST, so
    // an empty message) reaches both.
    assert_eq!(h2.request(Method::POST).await, (200, "sent=2".to_string()));
    assert_eq!(next_text(&mut http1).await, "");
    assert_eq!(next_text(&mut http2).await, "");

    // A browser origin that does not match the host is refused.
    assert_eq!(
        h2.websocket(Some("https://evil.example")).await.err(),
        Some(403)
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_dropped_connection_runs_on_close_for_each_websocket_stream() {
    let site = Site::start("drop");
    let mut h2 = H2Connection::connect(&site.address).await;
    let mut first = h2.websocket(None).await.expect("first");
    let mut second = h2.websocket(None).await.expect("second");
    assert!(next_text(&mut first).await.starts_with("open:"));
    assert!(next_text(&mut second).await.starts_with("open:"));

    // Cut the TCP connection under both streams: no close frame, no END_STREAM.
    h2.socket.shutdown(Shutdown::Both).expect("shutdown socket");
    let log = wait_for_lines(&site.root.join("close.log"), 2).await;
    assert_eq!(log, "close:1006\nclose:1006\n");
}
