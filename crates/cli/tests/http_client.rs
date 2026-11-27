//! `Phera\Http\Client` against real servers, and the `packages/phera-psr`
//! test suites:
//! - a `phera serve` router (`http_client/router.php`) for methods, JSON,
//!   headers, query strings, redirects, timeouts and `pool()`;
//! - a Hyper server in this test that numbers its connections, to see
//!   keep-alive reuse, HTTP/2 multiplexing, and a client made before the
//!   snapshot keeping its connection across forked requests;
//! - a TLS server with a self-signed certificate, for `verify` and ALPN.

use std::convert::Infallible;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::{ServerConfig, pki_types::PrivatePkcs8KeyDer};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/http_client")
}

fn package() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/phera-psr")
}

/// `phera serve` on a free port, stopped when dropped.
struct Served {
    process: Child,
    /// Kept open: the server may write more after its banner.
    _stdout: BufReader<ChildStdout>,
    address: String,
}

impl Served {
    fn start(script: &Path, environment: &[(&str, &str)]) -> Served {
        let mut command = Command::new(env!("CARGO_BIN_EXE_phera"));
        command
            .args(["serve", "--listen", "127.0.0.1:0", "--workers", "2"])
            .arg(script)
            .env_clear()
            .envs(environment.iter().copied())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut process = command.spawn().expect("start phera serve");
        let mut stdout = BufReader::new(process.stdout.take().expect("stdout"));
        let mut banner = String::new();
        stdout.read_line(&mut banner).expect("read the banner");
        let address = banner
            .split("http://")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or_else(|| panic!("no address in banner: {banner:?}"))
            .to_owned();
        Served {
            process,
            _stdout: stdout,
            address: format!("http://{address}"),
        }
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Runs `phera script arguments...`; returns its exit status and output.
async fn run(script: PathBuf, arguments: Vec<String>) -> (i32, String, String) {
    tokio::task::spawn_blocking(move || {
        let output = Command::new(env!("CARGO_BIN_EXE_phera"))
            .arg(&script)
            .args(&arguments)
            .env_clear()
            .output()
            .expect("run phera");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    })
    .await
    .expect("phera task")
}

/// A server that answers `conn=<number> version=<HTTP version> path=<path>`;
/// connections are numbered from 0 as they are accepted. `/sleep` waits
/// 300 ms first. Speaks HTTP/1.1 and HTTP/2 (h2c), or TLS with ALPN.
async fn numbering_server(tls: Option<TlsAcceptor>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let scheme = if tls.is_some() { "https" } else { "http" };
    let address = format!("{scheme}://{}", listener.local_addr().expect("address"));
    let connections = Arc::new(AtomicUsize::new(0));
    let counter = connections.clone();
    tokio::spawn(async move {
        loop {
            let Ok((socket, _)) = listener.accept().await else {
                return;
            };
            let number = counter.fetch_add(1, Ordering::SeqCst);
            let tls = tls.clone();
            tokio::spawn(async move {
                let service = service_fn(move |request: Request<Incoming>| async move {
                    if request.uri().path() == "/sleep" {
                        tokio::time::sleep(Duration::from_millis(300)).await;
                    }
                    let body = format!(
                        "conn={number} version={:?} path={}",
                        request.version(),
                        request.uri().path()
                    );
                    Ok::<_, Infallible>(Response::new(Full::new(Bytes::from(body))))
                });
                let builder = auto::Builder::new(TokioExecutor::new());
                match tls {
                    Some(acceptor) => {
                        if let Ok(stream) = acceptor.accept(socket).await {
                            let _ = builder
                                .serve_connection(TokioIo::new(stream), service)
                                .await;
                        }
                    }
                    None => {
                        let _ = builder
                            .serve_connection(TokioIo::new(socket), service)
                            .await;
                    }
                }
            });
        }
    });
    (address, connections)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_talks_to_a_router() {
    let server = Served::start(&fixtures().join("router.php"), &[]);
    let (status, output, errors) =
        run(fixtures().join("client.php"), vec![server.address.clone()]).await;
    assert_eq!(status, 0, "{errors}");
    let expected = [
        r#"get: [true,200,"Hello","text/plain; charset=utf-8","fallback"]"#,
        r#"post json: {"method":"POST","query":{"q":"a b","list":["1","2"]},"body":"{\"name\":\"Ada\"}","type":"application/json","tag":"override","agent":"Phera/0.1.0"}"#,
        r#"response headers: ["phera",["a=1","b=2"],"a=1, b=2"]"#,
        r#"put body: {"method":"PUT","query":[],"body":"raw","type":"text/plain","tag":"default","agent":"Phera/0.1.0"}"#,
        r#"patch: "PATCH""#,
        r#"delete: "DELETE""#,
        r#"request: "OPTIONS""#,
        r#"head: [200,""]"#,
        r#"decode object: "GET""#,
        r#"redirect: [200,"Hello"]"#,
        r#"303 becomes GET: "GET""#,
        r#"307 repeats: ["POST","kept"]"#,
        r#"redirects off: [302,"/hello"]"#,
        r#"per request: "Hello""#,
        r#"loop: ["Phera\\Http\\ClientException","Will not follow more than 5 redirects (last went to /loop)"]"#,
        r#"timeout: "Request timed out after 200 ms""#,
        r#"error status: [404,"status"]"#,
        r#"pool: {"first":"slept 300","second":"slept 300","7":"{\"method\":\"POST\",\"query\":[],\"body\":\"[1,2]\",\"type\":\"application/json\",\"tag\":\"default\",\"agent\":\"Phera/0.1.0\"}"}"#,
        r#"pool ran at once: true"#,
        r#"refused: true"#,
        r#"typo: "Phera\\Http\\Client: unknown option \"timeuot\"""#,
        r#"body and json: "Phera\\Http\\Client: give either \"body\" or \"json\", not both""#,
        r#"exception class: true"#,
    ];
    assert_eq!(output.lines().collect::<Vec<_>>(), expected, "{errors}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connections_are_kept_and_http2_requests_share_one() {
    let (address, connections) = numbering_server(None).await;
    let (status, output, errors) = run(fixtures().join("reuse.php"), vec![address]).await;
    assert_eq!(status, 0, "{errors}");
    assert_eq!(
        output.lines().collect::<Vec<_>>(),
        [
            "http1 conn=0 version=HTTP/1.1 path=/n/1",
            "http1 conn=0 version=HTTP/1.1 path=/n/2",
            "http1 conn=0 version=HTTP/1.1 path=/n/3",
            "http2 conn=1 version=HTTP/2.0 path=/sleep | conn=1 version=HTTP/2.0 path=/sleep | conn=1 version=HTTP/2.0 path=/sleep | conn=1 version=HTTP/2.0 path=/sleep",
            "multiplexed yes",
        ],
        "{errors}"
    );
    assert_eq!(connections.load(Ordering::SeqCst), 2);
}

/// A client made before the snapshot works in each forked request, and
/// the worker's connection to the upstream stays open between requests.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_made_at_boot_reuses_its_connection_across_requests() {
    let (upstream, connections) = numbering_server(None).await;
    let server = Served::start(&fixtures().join("proxy.php"), &[("UPSTREAM", &upstream)]);
    let address = server.address.clone();
    let bodies = tokio::task::spawn_blocking(move || {
        (1..=4)
            .map(|n| fetch(&format!("{address}/proxy/{n}")))
            .collect::<Vec<_>>()
    })
    .await
    .expect("requests");
    for (n, body) in bodies.iter().enumerate() {
        assert!(
            body.ends_with(&format!("version=HTTP/1.1 path=/n/{}", n + 1)),
            "{body}"
        );
    }
    // Two workers may each open one connection; never one per request.
    let opened = connections.load(Ordering::SeqCst);
    assert!(
        (1..=2).contains(&opened),
        "{opened} connections for 4 requests"
    );
}

/// One plain HTTP/1.1 GET with a fresh connection, returning the body.
fn fetch(url: &str) -> String {
    use std::io::{Read, Write};
    let rest = url.strip_prefix("http://").expect("http URL");
    let (host, path) = rest.split_once('/').expect("path");
    let mut socket = std::net::TcpStream::connect(host).expect("connect");
    write!(
        socket,
        "GET /{path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .expect("send");
    let mut response = String::new();
    socket.read_to_string(&mut response).expect("read");
    let (head, body) = response.split_once("\r\n\r\n").expect("a response");
    assert!(head.starts_with("HTTP/1.1 200"), "{head}");
    body.to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tls_verifies_certificates_and_negotiates_http2() {
    let certificate = rcgen::generate_simple_self_signed(vec!["127.0.0.1".to_owned()]).unwrap();
    let mut config = ServerConfig::builder_with_provider(phera_io_pool::tls::provider().clone())
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![certificate.cert.der().clone()],
            PrivatePkcs8KeyDer::from(certificate.signing_key.serialize_der()).into(),
        )
        .unwrap();
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let (address, _) = numbering_server(Some(TlsAcceptor::from(Arc::new(config)))).await;
    let (status, output, errors) = run(fixtures().join("tls.php"), vec![address]).await;
    assert_eq!(status, 0, "{errors}");
    let lines: Vec<_> = output.lines().collect();
    assert_eq!(lines.len(), 3, "{output}{errors}");
    assert!(
        lines[0].starts_with("unverified h2: conn=")
            && lines[0].ends_with("version=HTTP/2.0 path=/tls"),
        "{output}"
    );
    assert!(
        lines[1].starts_with("unverified h1: conn=")
            && lines[1].ends_with("version=HTTP/1.1 path=/tls"),
        "{output}"
    );
    assert_eq!(lines[2], "verified: refused", "{output}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn phera_psr_unit_suite_passes() {
    let (status, output, errors) = run(package().join("tests/unit.php"), Vec::new()).await;
    assert_eq!(status, 0, "{output}{errors}");
    assert!(output.ends_with(" passed, 0 failed\n"), "{output}{errors}");
}

/// PSR-18 through `Phera\Http\Client` to a PSR-15 application mounted on
/// `Phera\Http\Router` with `Psr15Bridge`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn phera_psr_serves_psr15_and_sends_psr18() {
    let server = Served::start(&package().join("tests/app.php"), &[]);
    let (status, output, errors) = run(
        package().join("tests/client.php"),
        vec![server.address.clone()],
    )
    .await;
    assert_eq!(status, 0, "{output}{errors}");
    assert!(output.ends_with(" passed, 0 failed\n"), "{output}{errors}");
}
