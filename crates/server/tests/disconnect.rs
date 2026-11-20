//! A client that leaves does not free the worker's connection slot until
//! its outstanding request finishes or reaches the request deadline.

mod common;

use std::future::{Future, pending};
use std::io::{ErrorKind, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::time::{Duration, Instant};

use http_body_util::Empty;
use hyper::body::Bytes;
use hyper_util::rt::{TokioExecutor, TokioIo};
use phera_server::{Handler, HandlerFactory, Request, Response, ServerConfig};
use tokio::sync::Semaphore;

const LIMIT: usize = 4;

struct Work {
    active: AtomicUsize,
    peak: AtomicUsize,
    finished: AtomicUsize,
    timed_out: AtomicUsize,
    release: Semaphore,
}

struct Active {
    work: Arc<Work>,
    waits_for_deadline: bool,
}

impl Drop for Active {
    fn drop(&mut self) {
        if self.waits_for_deadline {
            self.work.timed_out.fetch_add(1, SeqCst);
        }
        self.work.active.fetch_sub(1, SeqCst);
    }
}

struct Routes(Arc<Work>);

impl HandlerFactory for Routes {
    fn create(&self, _: usize) -> Rc<dyn Handler> {
        Rc::new(Self(self.0.clone()))
    }
}

impl Handler for Routes {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async move {
            let active = self.0.active.fetch_add(1, SeqCst) + 1;
            self.0.peak.fetch_max(active, SeqCst);
            let _active = Active {
                work: self.0.clone(),
                waits_for_deadline: request.path == "/timeout",
            };
            match request.path.as_str() {
                "/finish" => {
                    self.0.release.acquire().await.unwrap().forget();
                    self.0.finished.fetch_add(1, SeqCst);
                }
                "/timeout" => pending::<()>().await,
                _ => {}
            }
            Response::text(200, "done")
        })
    }
}

fn send(address: SocketAddr, path: &str) -> TcpStream {
    let mut client = TcpStream::connect(address).unwrap();
    write!(
        client,
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    client
}

fn wait_for(condition: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < until, "request work did not settle");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn disconnected_requests_keep_their_slot_and_deadline() {
    let work = Arc::new(Work {
        active: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
        finished: AtomicUsize::new(0),
        timed_out: AtomicUsize::new(0),
        release: Semaphore::new(0),
    });
    let (server, address) = common::start_with(
        Routes(work.clone()),
        ServerConfig {
            max_connections_per_worker: LIMIT,
            request_timeout: Some(Duration::from_secs(2)),
            ..common::test_config()
        },
    );
    let clients = [
        send(address, "/finish"),
        send(address, "/timeout"),
        send(address, "/finish"),
        send(address, "/timeout"),
    ];
    wait_for(|| work.active.load(SeqCst) == LIMIT);
    drop(clients);

    let mut waiting = send(address, "/probe");
    waiting
        .set_read_timeout(Some(Duration::from_millis(150)))
        .unwrap();
    let error = waiting.read(&mut [0]).unwrap_err();
    assert!(matches!(
        error.kind(),
        ErrorKind::WouldBlock | ErrorKind::TimedOut
    ));
    assert_eq!(work.active.load(SeqCst), LIMIT);
    assert_eq!(work.timed_out.load(SeqCst), 0);

    work.release.add_permits(2);
    waiting
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut response = String::new();
    waiting.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
    wait_for(|| work.active.load(SeqCst) == 0);
    assert_eq!(work.peak.load(SeqCst), LIMIT);
    assert_eq!(work.finished.load(SeqCst), 2);
    assert_eq!(work.timed_out.load(SeqCst), 2);
    server.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn reset_http2_streams_still_count_as_in_flight() {
    let work = Arc::new(Work {
        active: AtomicUsize::new(0),
        peak: AtomicUsize::new(0),
        finished: AtomicUsize::new(0),
        timed_out: AtomicUsize::new(0),
        release: Semaphore::new(0),
    });
    let (server, address) = common::start_with(
        Routes(work.clone()),
        ServerConfig {
            max_connections_per_worker: 2,
            request_timeout: Some(Duration::from_secs(2)),
            ..common::test_config()
        },
    );
    tokio::time::timeout(Duration::from_secs(6), async {
        let socket = tokio::net::TcpStream::connect(address).await.unwrap();
        let (mut client, connection) =
            hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(socket))
                .await
                .unwrap();
        let connection = tokio::spawn(connection);
        let request = |path: &str| {
            hyper::Request::post(format!("http://{address}{path}"))
                .body(Empty::<Bytes>::new())
                .unwrap()
        };
        let first = client.send_request(request("/finish"));
        let second = client.send_request(request("/timeout"));
        while work.active.load(SeqCst) != 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        drop((first, second));
        let excess = client.send_request(request("/probe")).await.unwrap();
        assert_eq!(excess.status(), hyper::StatusCode::SERVICE_UNAVAILABLE);

        work.release.add_permits(1);
        while work.active.load(SeqCst) != 1 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let admitted = client.send_request(request("/probe")).await.unwrap();
        assert_eq!(admitted.status(), hyper::StatusCode::OK);
        while work.active.load(SeqCst) != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(work.peak.load(SeqCst), 2);
        assert_eq!(work.finished.load(SeqCst), 1);
        assert_eq!(work.timed_out.load(SeqCst), 1);
        connection.abort();
    })
    .await
    .unwrap();
    server.shutdown();
}
