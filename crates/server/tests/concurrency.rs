//! Concurrency: many slow requests in flight on two workers.

mod common;

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::time::{Duration, Instant};

use common::Client;
use phera_server::{Handler, HandlerFactory, Request, Response};

/// A handler that pretends to wait for I/O.
struct Slow;

impl Handler for Slow {
    fn handle(&self, _request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_millis(200)).await;
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
fn many_slow_requests_run_concurrently() {
    let (server, addr) = common::start(SlowFactory, 2, 64 * 1024);
    let start = Instant::now();

    // Open every connection first, then fire every request, so the requests
    // are in flight together rather than spaced by connection setup.
    const REQUESTS: usize = 200;
    let mut clients: Vec<Client> = (0..REQUESTS).map(|_| Client::connect(addr)).collect();
    for client in &mut clients {
        client.send("GET /slow HTTP/1.1", &[], b"");
    }
    for client in &mut clients {
        let response = client.read_response();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"done");
    }

    let elapsed = start.elapsed();
    println!("200 concurrent requests on 2 workers took {elapsed:?}");
    // Sequential handling would need 200 * 200ms / 2 = 20 s.
    assert!(elapsed < Duration::from_secs(2), "took {elapsed:?}");
    server.shutdown();
}
