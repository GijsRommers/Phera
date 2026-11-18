//! The limits that keep one client from holding a worker: a time limit for
//! a request, for a stalled upload and for a silent connection, and a cap
//! on connections.

mod common;

use std::future::Future;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use common::Client;
use phera_server::{Handler, HandlerFactory, Request, Response, ServerConfig};

/// `/sleep` waits five seconds, `/spin` never finishes but yields like a
/// running PHP script does, anything else answers at once.
struct Slow {
    stopped: Arc<AtomicUsize>,
}

/// Counts the requests that were dropped before they finished.
struct CountIfStopped {
    stopped: Arc<AtomicUsize>,
    finished: bool,
}

impl Drop for CountIfStopped {
    fn drop(&mut self) {
        if !self.finished {
            self.stopped.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl Handler for Slow {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        let stopped = self.stopped.clone();
        Box::pin(async move {
            // Lives as long as the request: dropped early if it is stopped.
            let mut guard = CountIfStopped {
                stopped,
                finished: false,
            };
            match request.path.as_str() {
                "/sleep" => tokio::time::sleep(Duration::from_secs(5)).await,
                "/spin" => loop {
                    tokio::task::yield_now().await;
                },
                _ => {}
            }
            guard.finished = true;
            Response::text(200, "done\n")
        })
    }
}

struct SlowFactory {
    stopped: Arc<AtomicUsize>,
}

impl HandlerFactory for SlowFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(Slow {
            stopped: self.stopped.clone(),
        })
    }
}

fn start(
    config: ServerConfig,
) -> (
    phera_server::ServerHandle,
    std::net::SocketAddr,
    Arc<AtomicUsize>,
) {
    let stopped = Arc::new(AtomicUsize::new(0));
    let factory = SlowFactory {
        stopped: stopped.clone(),
    };
    let (server, addr) = common::start_with(factory, config);
    (server, addr, stopped)
}

const SHORT: Duration = Duration::from_millis(300);

#[test]
fn a_request_that_takes_too_long_is_stopped_and_answered_504() {
    let config = ServerConfig {
        request_timeout: Some(SHORT),
        ..common::test_config()
    };
    let (server, addr, stopped) = start(config);

    for (count, path) in ["/sleep", "/spin"].into_iter().enumerate() {
        let started = Instant::now();
        let response = Client::connect(addr).request(&format!("GET {path} HTTP/1.1"), &[], b"");
        assert_eq!(response.status, 504, "{path}");
        assert!(started.elapsed() < Duration::from_secs(2), "{path}");
        // The handler did not keep running behind the client's back.
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(stopped.load(Ordering::SeqCst), count + 1, "{path}");
    }

    // The worker is free again, and a quick request is not affected.
    let response = Client::connect(addr).request("GET / HTTP/1.1", &[], b"");
    assert_eq!(
        (response.status, response.body.as_slice()),
        (200, &b"done\n"[..])
    );
    assert_eq!(stopped.load(Ordering::SeqCst), 2);
    server.shutdown();
}

#[test]
fn without_a_time_limit_a_slow_request_finishes() {
    let config = ServerConfig {
        request_timeout: None,
        ..common::test_config()
    };
    let (server, addr, stopped) = start(config);
    let response = Client::connect(addr).request("GET /sleep HTTP/1.1", &[], b"");
    assert_eq!(response.status, 200);
    assert_eq!(stopped.load(Ordering::SeqCst), 0);
    server.shutdown();
}

#[test]
fn an_upload_that_stops_is_answered_408_and_the_connection_closed() {
    let config = ServerConfig {
        body_idle_timeout: SHORT,
        ..common::test_config()
    };
    let (server, addr, _) = start(config);
    let mut client = Client::connect(addr);
    let started = Instant::now();
    // Ten bytes promised, three sent.
    client.send("POST / HTTP/1.1", &["Content-Length: 10"], b"abc");
    let response = client.read_response();
    assert_eq!(response.status, 408);
    assert_eq!(response.header("connection"), Some("close"));
    assert!(started.elapsed() >= SHORT);
    assert!(started.elapsed() < Duration::from_secs(2));

    // An upload that arrives in pieces, each in time, is fine.
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .write_all(
            b"POST / HTTP/1.1\r\nHost: test\r\nConnection: close\r\nContent-Length: 6\r\n\r\n",
        )
        .unwrap();
    for piece in [b"ab", b"cd", b"ef"] {
        std::thread::sleep(Duration::from_millis(150));
        stream.write_all(piece).unwrap();
    }
    let mut answer = String::new();
    stream.read_to_string(&mut answer).unwrap();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    server.shutdown();
}

/// How long until the server closes `stream` without sending anything.
fn time_until_closed(stream: &mut TcpStream) -> Duration {
    let started = Instant::now();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut rest = Vec::new();
    stream
        .read_to_end(&mut rest)
        .expect("the server closes the connection");
    assert!(rest.is_empty(), "unexpected bytes: {rest:?}");
    started.elapsed()
}

#[test]
fn a_silent_connection_is_closed() {
    let config = ServerConfig {
        idle_timeout: SHORT,
        ..common::test_config()
    };
    let (server, addr, _) = start(config);

    // Never sends anything.
    let mut silent = TcpStream::connect(addr).unwrap();
    let waited = time_until_closed(&mut silent);
    assert!(
        waited >= SHORT && waited < Duration::from_secs(2),
        "{waited:?}"
    );

    // Starts a request and never finishes its headers.
    let mut unfinished = TcpStream::connect(addr).unwrap();
    unfinished.write_all(b"GET / HTTP/1.1\r\nHost: te").unwrap();
    let waited = time_until_closed(&mut unfinished);
    assert!(waited < Duration::from_secs(2), "{waited:?}");

    // Idle after a request that was answered.
    let mut idle = TcpStream::connect(addr).unwrap();
    idle.write_all(b"GET / HTTP/1.1\r\nHost: test\r\n\r\n")
        .unwrap();
    let mut answer = [0u8; 512];
    let read = idle.read(&mut answer).unwrap();
    assert!(answer[..read].starts_with(b"HTTP/1.1 200"));
    let waited = time_until_closed(&mut idle);
    assert!(
        waited >= SHORT && waited < Duration::from_secs(2),
        "{waited:?}"
    );
    server.shutdown();
}

/// With room for two connections, a third client is not served until one
/// of the first two leaves. It is not refused either: it waits its turn.
#[test]
fn connections_beyond_the_limit_wait_their_turn() {
    let config = ServerConfig {
        max_connections_per_worker: 2,
        ..common::test_config()
    };
    let (server, addr, _) = start(config);
    let mut first = Client::connect(addr);
    let mut second = Client::connect(addr);
    assert_eq!(first.request("GET / HTTP/1.1", &[], b"").status, 200);
    assert_eq!(second.request("GET / HTTP/1.1", &[], b"").status, 200);

    let mut third = TcpStream::connect(addr).unwrap();
    third
        .write_all(b"GET / HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n")
        .unwrap();
    third.set_read_timeout(Some(SHORT)).unwrap();
    let mut answer = [0u8; 512];
    let waiting = third.read(&mut answer);
    assert!(waiting.is_err(), "the third client was served: {waiting:?}");

    drop(first);
    third
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let read = third
        .read(&mut answer)
        .expect("an answer once there is room");
    assert!(answer[..read].starts_with(b"HTTP/1.1 200"));
    drop(second);
    server.shutdown();
}
