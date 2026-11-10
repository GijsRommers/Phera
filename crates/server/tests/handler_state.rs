//! Non-Send handler state: a per-worker `Rc<Cell<u64>>` counter.

mod common;

use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use common::Client;
use phera_server::{Handler, HandlerFactory, Request, Response};

struct Counter {
    /// Not `Send`: proving it works is the point of this test.
    count: Rc<Cell<u64>>,
}

impl Handler for Counter {
    fn handle(&self, _request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async move {
            self.count.set(self.count.get() + 1);
            Response::text(200, &self.count.get().to_string())
        })
    }
}

struct CounterFactory;

impl HandlerFactory for CounterFactory {
    // One counter per worker, created on that worker's thread.
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(Counter {
            count: Rc::new(Cell::new(0)),
        })
    }
}

#[test]
fn per_worker_counter_increments() {
    // One worker so the sequence is deterministic.
    let (server, addr) = common::start(CounterFactory, 1, 64 * 1024);
    let mut client = Client::connect(addr);
    for expected in 1..=5u64 {
        let response = client.request("GET /count HTTP/1.1", &["Connection: close"], b"");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, expected.to_string().as_bytes());
    }
    server.shutdown();
}
