//! Graceful shutdown: in-flight requests finish, the port stops accepting.

mod common;

use std::future::Future;
use std::net::TcpStream;
use std::pin::Pin;
use std::rc::Rc;
use std::thread;
use std::time::{Duration, Instant};

use common::Client;
use phera_server::{Handler, HandlerFactory, Request, Response};

struct Slow;

impl Handler for Slow {
    fn handle(&self, _request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_millis(400)).await;
            Response::text(200, "done")
        })
    }
}

struct SlowFactory;

impl HandlerFactory for SlowFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(Slow)
    }
}

#[test]
fn graceful_shutdown_completes_in_flight_requests() {
    let (server, addr) = common::start(SlowFactory, 1, 64 * 1024);
    let mut client = Client::connect(addr);
    client.send("GET /slow HTTP/1.1", &[], b"");

    // The request is in flight when the shutdown starts.
    thread::sleep(Duration::from_millis(100));
    let start = Instant::now();
    server.shutdown();
    let shutdown_took = start.elapsed();
    assert!(
        shutdown_took < Duration::from_secs(3),
        "shutdown waited too long: {shutdown_took:?}"
    );

    // The in-flight request still got its response.
    let response = client.read_response();
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"done");

    // And the port has stopped accepting.
    assert!(
        TcpStream::connect(addr).is_err(),
        "port still accepts connections"
    );
}
