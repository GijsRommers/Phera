//! A panicking handler must not take the worker down.

mod common;

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use common::Client;
use phera_server::{Handler, HandlerFactory, Request, Response};

struct Floppy;

impl Handler for Floppy {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async move {
            if request.path == "/panic" {
                panic!("boom");
            }
            Response::text(200, "ok")
        })
    }
}

struct FloppyFactory;

impl HandlerFactory for FloppyFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(Floppy)
    }
}

#[test]
fn panic_does_not_kill_the_worker() {
    // One worker: if the panic killed it, nothing would answer afterwards.
    let (server, addr) = common::start(FloppyFactory, 1, 64 * 1024);
    let mut client = Client::connect(addr);

    let response = client.request("GET /panic HTTP/1.1", &["Connection: close"], b"");
    assert_eq!(response.status, 500);

    for _ in 0..3 {
        let response = client.request("GET /ok HTTP/1.1", &["Connection: close"], b"");
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"ok");
    }
    server.shutdown();
}
