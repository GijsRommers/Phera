//! Keep-alive: two requests over one connection.

mod common;

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use common::Client;
use phera_server::{Handler, HandlerFactory, Request, Response};

struct Routes;

impl Handler for Routes {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async move { Response::text(200, &format!("path={}", request.path)) })
    }
}

struct RoutesFactory;

impl HandlerFactory for RoutesFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(Routes)
    }
}

#[test]
fn two_requests_on_one_connection() {
    let (server, addr) = common::start(RoutesFactory, 2, 64 * 1024);
    let mut client = Client::connect(addr);

    let first = client.request("GET /one HTTP/1.1", &[], b"");
    assert_eq!(first.status, 200);
    assert_eq!(first.body, b"path=/one");
    assert_ne!(first.header("connection"), Some("close"));

    let second = client.request("GET /two HTTP/1.1", &[], b"");
    assert_eq!(second.status, 200);
    assert_eq!(second.body, b"path=/two");

    server.shutdown();
}
