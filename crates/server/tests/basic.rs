//! GET responses, repeated headers, and HEAD without a body.

mod common;

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use common::Client;
use phera_server::{Handler, HandlerFactory, Request, Response};

struct Routes;

impl Handler for Routes {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async move {
            match request.path.as_str() {
                "/cookies" => Response::new(200)
                    .with_header("set-cookie", "first=1; Path=/")
                    .with_header("set-cookie", "second=2; Path=/")
                    .with_body(b"cookies".to_vec()),
                "/doc" => Response::new(200)
                    .with_header("content-type", "text/html")
                    .with_body(b"0123456789".to_vec()),
                _ => Response::text(200, &format!("path={}", request.path)),
            }
        })
    }
}

struct RoutesFactory;

impl HandlerFactory for RoutesFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(Routes)
    }
}

#[test]
fn get_returns_status_headers_and_body() {
    let (server, addr) = common::start(RoutesFactory, 2, 64 * 1024);
    let mut client = Client::connect(addr);
    let response = client.request("GET /hello?x=1 HTTP/1.1", &["Connection: close"], b"");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"path=/hello");
    assert_eq!(
        response.header("content-type"),
        Some("text/plain; charset=utf-8")
    );
    // The server is responsible for Content-Length and Date.
    assert_eq!(response.header("content-length"), Some("11"));
    assert!(
        response.header("date").is_some(),
        "response has no Date header"
    );
    server.shutdown();
}

#[test]
fn repeated_header_names_survive() {
    let (server, addr) = common::start(RoutesFactory, 2, 64 * 1024);
    let mut client = Client::connect(addr);
    let response = client.request("GET /cookies HTTP/1.1", &["Connection: close"], b"");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, b"cookies");
    assert_eq!(
        response.headers_all("set-cookie"),
        vec!["first=1; Path=/", "second=2; Path=/"]
    );
    server.shutdown();
}

#[test]
fn head_has_content_length_but_no_body() {
    let (server, addr) = common::start(RoutesFactory, 2, 64 * 1024);
    let mut client = Client::connect(addr);
    client.send("HEAD /doc HTTP/1.1", &[], b"");
    let response = client.read_response_head();
    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-length"), Some("10"));
    client.assert_no_body();
    server.shutdown();
}
