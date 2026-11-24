//! Tests the same request and limit cases over HTTP/1.1 and HTTP/2 (`h2c`
//! with prior knowledge) on one `phera_server::Server` listener.

mod common;

use std::future::Future;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body as HyperBody, Frame, Incoming};
use hyper::client::conn::{http1, http2};
use hyper::header::{HeaderName, HeaderValue};
use hyper::{Request as HyperRequest, Response as HyperResponse};
use hyper_util::rt::{TokioExecutor, TokioIo};
use phera_server::{Handler, HandlerFactory, HttpVersion, Request, Response, ServerConfig};
use tokio::net::TcpStream;

#[derive(Clone, Copy)]
struct EchoAndSlowFactory;

impl HandlerFactory for EchoAndSlowFactory {
    fn create(&self, _worker_index: usize) -> Rc<dyn Handler> {
        Rc::new(EchoAndSlow)
    }
}

struct EchoAndSlow;

impl Handler for EchoAndSlow {
    fn handle(&self, request: Request) -> Pin<Box<dyn Future<Output = Response> + '_>> {
        Box::pin(async move {
            match request.path.as_str() {
                "/slow" => {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    Response::text(200, "slow\n")
                }
                "/hang" => {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    Response::text(200, "never\n")
                }
                "/hop" => Response::text(200, "ok\n")
                    .with_header("connection", "close")
                    .with_header("keep-alive", "timeout=5"),
                _ => {
                    let proto = match request.version {
                        HttpVersion::Http10 => "HTTP/1.0",
                        HttpVersion::Http11 => "HTTP/1.1",
                        HttpVersion::Http2 => "HTTP/2.0",
                    };
                    let host = request
                        .header("host")
                        .map(|v| String::from_utf8_lossy(v).into_owned())
                        .unwrap_or_default();
                    let extra = request
                        .header("x-test")
                        .map(|v| String::from_utf8_lossy(v).into_owned())
                        .unwrap_or_default();
                    let body = String::from_utf8_lossy(&request.body);
                    let summary = format!(
                        "{proto} {} {} host={host} x-test={extra} body={body}",
                        request.method, request.target
                    );
                    Response::text(200, &summary)
                        .with_header("set-cookie", "a=1")
                        .with_header("set-cookie", "b=2")
                }
            }
        })
    }
}

#[derive(Clone, Copy)]
enum Proto {
    Http1,
    Http2,
}

async fn send_full(
    addr: SocketAddr,
    proto: Proto,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &'static [u8],
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let stream = TcpStream::connect(addr).await.unwrap();
    let io = TokioIo::new(stream);
    let mut builder = HyperRequest::builder().method(method).uri(target);
    if proto_is_h1(proto) {
        builder = builder.header("host", addr.to_string());
    } else {
        builder = HyperRequest::builder()
            .method(method)
            .uri(format!("http://{addr}{target}"));
    }
    for (name, value) in headers {
        builder = builder.header(
            HeaderName::try_from(*name).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }
    let req = builder.body(Full::new(Bytes::from_static(body))).unwrap();

    let resp: HyperResponse<Incoming> = match proto {
        Proto::Http1 => {
            let (mut sender, conn) = http1::handshake(io).await.unwrap();
            tokio::spawn(async move {
                let _ = conn.await;
            });
            sender.send_request(req).await.unwrap()
        }
        Proto::Http2 => {
            let (mut sender, conn) = http2::Builder::new(TokioExecutor::new())
                .handshake(io)
                .await
                .unwrap();
            tokio::spawn(async move {
                let _ = conn.await;
            });
            sender.send_request(req).await.unwrap()
        }
    };

    let status = resp.status().as_u16();
    let resp_headers = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap().to_string()))
        .collect();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, resp_headers, bytes)
}

fn proto_is_h1(proto: Proto) -> bool {
    matches!(proto, Proto::Http1)
}

#[tokio::test(flavor = "current_thread")]
async fn same_requests_work_over_http1_and_http2() {
    let (server, addr) = common::start(EchoAndSlowFactory, 2, 1024);

    for (proto, expected_proto) in [(Proto::Http1, "HTTP/1.1"), (Proto::Http2, "HTTP/2.0")] {
        let (status, headers, body) = send_full(
            addr,
            proto,
            "POST",
            "/items?page=2",
            &[("x-test", "hello")],
            b"payload",
        )
        .await;
        assert_eq!(status, 200);
        let text = String::from_utf8(body).unwrap();
        assert_eq!(
            text,
            format!("{expected_proto} POST /items?page=2 host={addr} x-test=hello body=payload")
        );
        let cookies: Vec<&str> = headers
            .iter()
            .filter(|(k, _)| k == "set-cookie")
            .map(|(_, v)| v.as_str())
            .collect();
        assert_eq!(cookies, vec!["a=1", "b=2"]);

        let (hop_status, hop_headers, hop_body) =
            send_full(addr, proto, "GET", "/hop", &[], b"").await;
        assert_eq!((hop_status, hop_body.as_slice()), (200, &b"ok\n"[..]));
        if !proto_is_h1(proto) {
            assert!(
                !hop_headers
                    .iter()
                    .any(|(k, _)| k == "connection" || k == "keep-alive"),
                "HTTP/2 response kept hop-by-hop headers: {hop_headers:?}"
            );
        }
    }

    server.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn http2_multiplexes_streams_on_one_connection() {
    let (server, addr) = common::start(EchoAndSlowFactory, 1, 1024);
    let stream = TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = http2::Builder::new(TokioExecutor::new())
        .handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let slow_req = HyperRequest::builder()
        .method("GET")
        .uri(format!("http://{addr}/slow"))
        .body(Full::new(Bytes::new()))
        .unwrap();
    let fast_req = HyperRequest::builder()
        .method("GET")
        .uri(format!("http://{addr}/fast"))
        .body(Full::new(Bytes::new()))
        .unwrap();

    let slow_fut = sender.send_request(slow_req);
    let fast_fut = sender.send_request(fast_req);
    let (slow_res, fast_res) = tokio::join!(slow_fut, fast_fut);
    assert_eq!(slow_res.unwrap().status().as_u16(), 200);
    assert_eq!(fast_res.unwrap().status().as_u16(), 200);

    server.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn http2_enforces_body_size_request_timeout_and_body_idle() {
    let config = ServerConfig {
        max_body_bytes: 16,
        request_timeout: Some(Duration::from_millis(250)),
        body_idle_timeout: Duration::from_millis(250),
        ..common::test_config()
    };
    let (server, addr) = common::start_with(EchoAndSlowFactory, config);

    let (status, _, _) = send_full(addr, Proto::Http2, "POST", "/", &[], &[b'x'; 32]).await;
    assert_eq!(status, 413);

    let (status, _, _) = send_full(addr, Proto::Http2, "GET", "/hang", &[], b"").await;
    assert_eq!(status, 504);

    // Stalled HTTP/2 upload triggers 408 without Connection: close.
    let stream = TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) = http2::Builder::new(TokioExecutor::new())
        .handshake(TokioIo::new(stream))
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let req = HyperRequest::builder()
        .method("POST")
        .uri(format!("http://{addr}/"))
        .body(StalledBody { sent: false })
        .unwrap();
    let resp = sender.send_request(req).await.unwrap();
    assert_eq!(resp.status().as_u16(), 408);
    assert!(!resp.headers().contains_key("connection"));

    server.shutdown();
}

#[test]
fn partial_http2_preface_that_stalls_closes_without_http1_408() {
    let config = ServerConfig {
        idle_timeout: Duration::from_millis(300),
        request_read_timeout: Duration::from_millis(150),
        ..common::test_config()
    };
    let (server, addr) = common::start_with(EchoAndSlowFactory, config);

    let mut stream = StdTcpStream::connect(addr).unwrap();
    stream.write_all(b"PRI * HTTP/2.0\r\n").unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let started = Instant::now();
    let mut rest = Vec::new();
    stream.read_to_end(&mut rest).unwrap();
    assert!(rest.is_empty(), "unexpected HTTP/1 response: {rest:?}");
    assert!(started.elapsed() < Duration::from_secs(2));

    server.shutdown();
}

struct StalledBody {
    sent: bool,
}

impl HyperBody for StalledBody {
    type Data = Bytes;
    type Error = std::convert::Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        if !self.sent {
            self.sent = true;
            Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(b"abc")))))
        } else {
            Poll::Pending
        }
    }
}
