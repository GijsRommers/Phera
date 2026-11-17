//! Request bodies: intact delivery and the size limit.

mod common;

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use common::Client;
use phera_server::{Handler, HandlerFactory, Request, Response};

struct Echo;

impl Handler for Echo {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async move {
            Response::new(200)
                .with_header("content-type", "application/octet-stream")
                .with_body(request.body)
        })
    }
}

struct EchoFactory;

impl HandlerFactory for EchoFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(Echo)
    }
}

#[test]
fn post_body_arrives_intact() {
    let (server, addr) = common::start(EchoFactory, 2, 64 * 1024);
    let mut client = Client::connect(addr);
    // Binary bytes, not just text, to catch any encoding on the way.
    let body: Vec<u8> = (0..=255u8).cycle().take(4096).collect();
    let content_length = format!("Content-Length: {}", body.len());
    let response = client.request("POST /echo HTTP/1.1", &[&content_length], &body);
    assert_eq!(response.status, 200);
    assert_eq!(response.body, body);
    server.shutdown();
}

#[test]
fn oversized_body_gets_413() {
    let (server, addr) = common::start(EchoFactory, 2, 16);
    let mut client = Client::connect(addr);
    let body = vec![b'a'; 100];
    let response = client.request("POST /echo HTTP/1.1", &["Content-Length: 100"], &body);
    assert_eq!(response.status, 413);
    server.shutdown();
}

#[test]
fn body_at_the_limit_is_accepted() {
    let (server, addr) = common::start(EchoFactory, 2, 16);
    let mut client = Client::connect(addr);
    let response = client.request(
        "POST /echo HTTP/1.1",
        &["Content-Length: 16"],
        b"0123456789abcdef",
    );
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"0123456789abcdef");
    server.shutdown();
}
