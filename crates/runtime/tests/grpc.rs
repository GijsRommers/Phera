//! `Phera\Grpc\Server` and `Phera\Grpc\Client` talking HTTP/2 to each
//! other: each script runs on a thread of its own, as two `phera`
//! processes would, over a real socket (`tests/grpc/*.php`).

use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use phera_runtime::Runtime;

const EXPECTED: &str = r#"raw: "cba"
say: {"message":"hi","index":3,"user":"alice"}
say headers: {"x-handler":"say"}
say trailers: ["2","000102"]
count: [{"message":"n","index":1,"user":""},{"message":"n","index":2,"user":""},{"message":"n","index":3,"user":""}]
list: [{"message":"","index":1,"user":""},{"message":"","index":2,"user":""}]
broken: ABORTED "stream broke" []
sum: {"total":42}
fail: NOT_FOUND "no echo for x: 100% gone" {"x-reason":"missing"}
crash: UNKNOWN "boom" []
unknown: UNIMPLEMENTED "Method not found: \/echo.Echo\/Nope" []
slow: DEADLINE_EXCEEDED "Deadline Exceeded" []
meta: ["\/echo.Echo\/Meta",{"x-a":"1","x-b":"2,3"},true,true]
gzip: true
health: "0801"
health service: "0801"
health unknown: NOT_FOUND "unknown service abc" []
bad request: "echo.Req: message echo.Req has no field \"nope\""
stop: "bye"
"#;

fn scripts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/grpc")
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .expect("a free port")
}

fn wait_for(address: SocketAddr) {
    let started = Instant::now();
    while TcpStream::connect_timeout(&address, Duration::from_millis(100)).is_err() {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the server did not start"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Runs the server script in the background and the client script against
/// it; returns both outputs once the client has stopped the server.
fn converse(endpoint: &str, port: u16, tls: &str) -> (String, String) {
    let server_script = scripts().join("server.php");
    let server_arguments = vec![format!("127.0.0.1:{port}"), tls.to_owned()];
    let server = std::thread::spawn(move || {
        Runtime::new()
            .run_file_blocking(&server_script, &server_arguments, None)
            .expect("server script runs")
    });
    wait_for(SocketAddr::from(([127, 0, 0, 1], port)));
    let client = Runtime::new()
        .run_file_blocking(
            scripts().join("client.php"),
            &[endpoint.to_owned(), tls.to_owned()],
            None,
        )
        .expect("client script runs");
    let server = server.join().expect("server thread");
    (
        String::from_utf8_lossy(&client.output).into_owned(),
        String::from_utf8_lossy(&server.output).into_owned(),
    )
}

#[test]
fn server_and_client_over_cleartext_http2() {
    let port = free_port();
    let (client, server) = converse(&format!("127.0.0.1:{port}"), port, "-");
    assert_eq!(client, EXPECTED);
    assert_eq!(server, "listening\nstopped\n");
}

#[test]
fn server_and_client_over_tls() {
    let directory = tempfile::tempdir().expect("temporary directory");
    let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_owned()])
        .expect("self-signed certificate");
    std::fs::write(directory.path().join("cert.pem"), certified.cert.pem()).unwrap();
    std::fs::write(
        directory.path().join("key.pem"),
        certified.signing_key.serialize_pem(),
    )
    .unwrap();
    let port = free_port();
    let tls = directory.path().to_string_lossy().into_owned();
    let (client, server) = converse(&format!("https://localhost:{port}"), port, &tls);
    assert_eq!(client, EXPECTED);
    assert_eq!(server, "listening\nstopped\n");
}
