//! Loopback server used by PHP and Phera. No oracle case uses external networking.

use std::{
    convert::Infallible,
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};

use bytes::Bytes;
use flate2::{Compression, write::GzEncoder};
use http_body_util::{BodyExt, Full, StreamBody, combinators::UnsyncBoxBody};
use hyper::{
    Request, Response,
    body::{Frame, Incoming},
    server::conn::{http1, http2},
    service::service_fn,
};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls::pki_types::PrivatePkcs8KeyDer;
use tokio::{net::TcpListener, sync::oneshot};
use tokio_rustls::TlsAcceptor;

type Body = UnsyncBoxBody<Bytes, std::io::Error>;

pub struct Server {
    pub address: SocketAddr,
    pub tls_address: SocketAddr,
    pub ca: PathBuf,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl Server {
    pub fn start(directory: &Path) -> Self {
        let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()]).unwrap();
        let ca = directory.join("ca.pem");
        std::fs::write(&ca, cert.cert.pem()).unwrap();
        let mut tls = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![cert.cert.der().clone()],
            PrivatePkcs8KeyDer::from(cert.signing_key.serialize_der()).into(),
        )
        .unwrap();
        tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let acceptor = TlsAcceptor::from(Arc::new(tls));
        let tcp = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let ssl = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        tcp.set_nonblocking(true).unwrap();
        ssl.set_nonblocking(true).unwrap();
        let address = tcp.local_addr().unwrap();
        let tls_address = ssl.local_addr().unwrap();
        let (shutdown, stop) = oneshot::channel();
        let task = thread::spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let tcp = TcpListener::from_std(tcp).unwrap();
                    let ssl = TcpListener::from_std(ssl).unwrap();
                    let mut stop = stop;
                    loop {
                        tokio::select! {
                            _ = &mut stop => break,
                            Ok((stream, _)) = tcp.accept() => {
                                tokio::spawn(async move {
                                    let _ = http1::Builder::new().serve_connection(
                                        TokioIo::new(stream), service_fn(respond),
                                    ).await;
                                });
                            }
                            Ok((stream, _)) = ssl.accept() => {
                                let acceptor = acceptor.clone();
                                tokio::spawn(async move {
                                    let Ok(tls) = acceptor.accept(stream).await else { return };
                                    let h2 = tls.get_ref().1.alpn_protocol() == Some(b"h2");
                                    let io = TokioIo::new(tls);
                                    if h2 {
                                        let _ = http2::Builder::new(TokioExecutor::new())
                                            .serve_connection(io, service_fn(respond)).await;
                                    } else {
                                        let _ = http1::Builder::new()
                                            .serve_connection(io, service_fn(respond)).await;
                                    }
                                });
                            }
                        }
                    }
                });
        });
        Self {
            address,
            tls_address,
            ca,
            shutdown: Some(shutdown),
            task: Some(task),
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(task) = self.task.take() {
            task.join().unwrap();
        }
    }
}

fn full(data: impl Into<Bytes>) -> Body {
    Full::new(data.into())
        .map_err(|never| match never {})
        .boxed_unsync()
}

async fn respond(request: Request<Incoming>) -> Result<Response<Body>, Infallible> {
    let path = request.uri().path();
    let mut response = Response::builder().header("date", "Sat, 01 Jan 2000 00:00:00 GMT");
    let data = match path {
        "/redirect" | "/redirect2" | "/redirect-307" => {
            let target = if path == "/redirect" {
                "/redirect2"
            } else {
                "/echo"
            };
            let code = if path == "/redirect-307" { 307 } else { 302 };
            response = response.status(code).header("location", target);
            full("redirect body")
        }
        "/cookie" => {
            response = response.header("set-cookie", "test=sample; Path=/; HttpOnly");
            full("cookie")
        }
        "/wait" => {
            tokio::time::sleep(Duration::from_millis(120)).await;
            full("waited")
        }
        "/fail" => {
            response = response.status(403);
            full("forbidden")
        }
        "/gzip" => {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(b"decompressed content").unwrap();
            response = response.header("content-encoding", "gzip");
            full(encoder.finish().unwrap())
        }
        "/chunks" => chunks(false),
        "/cut" => {
            response = response.header("content-length", 100);
            chunks(true)
        }
        "/large" => full(Bytes::from(vec![b'x'; 2 * 1024 * 1024])),
        "/multipart" => {
            let (parts, body) = request.into_parts();
            let media = parts.headers.get("content-type").unwrap().to_str().unwrap();
            let boundary = media.split_once("boundary=").unwrap().1;
            let body = body.collect().await.unwrap().to_bytes();
            let text = String::from_utf8_lossy(&body).replace(boundary, "BOUNDARY");
            full(text)
        }
        _ => {
            let (parts, body) = request.into_parts();
            let mut echo = format!("{}|{}", parts.method, parts.uri.path()).into_bytes();
            for name in [
                "content-type",
                "x-test",
                "cookie",
                "authorization",
                "referer",
            ] {
                echo.push(b'|');
                if let Some(value) = parts.headers.get(name) {
                    echo.extend_from_slice(value.as_bytes());
                }
            }
            echo.push(b'|');
            echo.extend_from_slice(&body.collect().await.unwrap().to_bytes());
            response = response.header("content-type", "application/octet-stream");
            full(echo)
        }
    };
    Ok(response.body(data).unwrap())
}

fn chunks(cut: bool) -> Body {
    StreamBody::new(futures_util::stream::unfold(0u8, move |step| async move {
        match step {
            0 => Some((Ok(Frame::data(Bytes::from_static(b"one"))), 1)),
            1 => {
                tokio::time::sleep(Duration::from_millis(10)).await;
                let frame = if cut {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionAborted,
                        "test cut",
                    ))
                } else {
                    Ok(Frame::data(Bytes::from_static(b"two")))
                };
                Some((frame, 2))
            }
            _ => None,
        }
    }))
    .boxed_unsync()
}
