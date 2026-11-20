//! A real client reads `flush()` output while its PHP request is still running.

use std::io::{BufRead, BufReader, Write};
use std::net::{Shutdown, TcpStream};
use std::path::Path;
use std::time::{Duration, Instant};

use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper_util::rt::{TokioExecutor, TokioIo};
use phera_extension_api::testing::serve::Server;
use tempfile::TempDir;

const WAIT_FOR_CLIENT: &str = r#"<?php
function wait_for_client(): void {
    for ($left = 500; $left > 0; --$left) {
        if (file_exists(__DIR__ . '/continue')) return;
        usleep(10000);
    }
    throw new RuntimeException('client never continued');
}
"#;

fn serve(script: &str) -> (TempDir, Server) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("index.php"), script).unwrap();
    let server = Server::start(Path::new(env!("CARGO_BIN_EXE_phera")), root.path(), 1, &[]);
    (root, server)
}

fn start_request(server: &Server, target: &str) -> BufReader<TcpStream> {
    let mut stream = TcpStream::connect(&server.address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    write!(
        stream,
        "GET {target} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    BufReader::new(stream)
}

fn head(client: &mut impl BufRead) -> String {
    let mut head = String::new();
    loop {
        let mut line = String::new();
        assert_ne!(client.read_line(&mut line).unwrap(), 0);
        if line == "\r\n" {
            return head.to_ascii_lowercase();
        }
        head.push_str(&line);
    }
}

fn chunk(client: &mut impl BufRead) -> Vec<u8> {
    let mut size = String::new();
    client.read_line(&mut size).unwrap();
    let size = usize::from_str_radix(size.trim_end(), 16).unwrap();
    let mut bytes = vec![0; size];
    client.read_exact(&mut bytes).unwrap();
    let mut end = [0; 2];
    client.read_exact(&mut end).unwrap();
    assert_eq!(&end, b"\r\n");
    bytes
}

#[test]
fn the_first_flush_sends_headers_and_body_over_http1() {
    for (mode, put_first, first) in [
        ("explicit", "echo 'first'; flush();", &b"first"[..]),
        (
            "implicit",
            "ob_implicit_flush(); echo 'first';",
            &b"first"[..],
        ),
        (
            "ob-flush",
            "ob_start(); echo 'first'; ob_flush();",
            &b"first"[..],
        ),
        (
            "ob-end-flush",
            "ob_start(); echo 'first'; ob_end_flush();",
            &b"first"[..],
        ),
        ("head", "flush();", &b""[..]),
    ] {
        let script = format!(
            r#"{WAIT_FOR_CLIENT}
header('X-Stage: early');
header_register_callback(fn () => header('X-Callback: ran'));
{put_first}
wait_for_client();
echo headers_sent() ? 'sent' : 'open';
header('X-Stage: late');
"#
        );
        let (root, server) = serve(&script);
        let mut client = start_request(&server, "/");
        let headers = head(&mut client);
        assert!(headers.starts_with("http/1.1 200"), "{mode}: {headers}");
        assert!(headers.contains("x-stage: early\r\n"), "{mode}: {headers}");
        assert!(headers.contains("x-callback: ran\r\n"), "{mode}: {headers}");
        assert!(
            headers.contains("transfer-encoding: chunked\r\n"),
            "{mode}: {headers}"
        );
        if !first.is_empty() {
            assert_eq!(chunk(&mut client), first, "{mode}");
        }
        std::fs::write(root.path().join("continue"), "").unwrap();
        assert_eq!(chunk(&mut client), b"sent", "{mode}");
        assert!(chunk(&mut client).is_empty(), "{mode}");
    }
}

#[test]
fn a_buffered_response_keeps_its_exact_length() {
    let (_root, server) = serve("<?php ob_start(); ob_start(); echo 'buffered'; ob_flush();");
    let response = server.get_one("/");
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    assert!(!head.contains("transfer-encoding:"));
    assert!(head.contains("content-length: 8"));
    assert_eq!(body, "buffered");
}

#[tokio::test(flavor = "current_thread")]
async fn flush_sends_data_frames_over_http2() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let (root, server) = serve(&format!(
            r#"{WAIT_FOR_CLIENT}
echo $_SERVER['SERVER_PROTOCOL'], ' ', $_SERVER['HTTP_HOST'];
flush();
wait_for_client();
echo 'done';
"#
        ));
        let socket = tokio::net::TcpStream::connect(&server.address)
            .await
            .unwrap();
        let (mut client, connection) =
            hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(socket))
                .await
                .unwrap();
        let connection = tokio::spawn(connection);
        let request = hyper::Request::get(format!("http://{}/", server.address))
            .body(Empty::<Bytes>::new())
            .unwrap();
        let response = client.send_request(request).await.unwrap();
        assert_eq!(response.version(), hyper::Version::HTTP_2);
        assert!(response.headers().get("transfer-encoding").is_none());
        assert!(response.headers().get("content-length").is_none());
        let mut body = response.into_body();

        let first = body.frame().await.unwrap().unwrap().into_data().unwrap();
        assert_eq!(first, format!("HTTP/2.0 {}", server.address));
        std::fs::write(root.path().join("continue"), "").unwrap();
        assert_eq!(body.collect().await.unwrap().to_bytes(), "done");
        connection.abort();
    })
    .await
    .expect("the first frame arrived while PHP was waiting for the client");
}

#[test]
fn finish_closes_the_response_and_keeps_the_script_running() {
    let (root, server) = serve(&format!(
        r#"{WAIT_FOR_CLIENT}
ob_start(fn ($body, $flags) => "$body:$flags");
echo 'response';
$finish = $_GET['function'];
$started = $finish();
wait_for_client();
echo 'late output';
register_shutdown_function(function () use ($started, $finish) {{
    $report = [$started, $finish(), ob_get_level(), headers_sent(), connection_status()];
    file_put_contents(__DIR__ . '/status', json_encode($report));
}});
"#
    ));
    for function in ["fastcgi_finish_request", "litespeed_finish_request"] {
        let status = root.path().join("status");
        let marker = root.path().join("continue");
        let response = server.get_one(&format!("/?function={function}"));
        assert_eq!(response.split_once("\r\n\r\n").unwrap().1, "response:9");
        assert!(!status.exists());
        std::fs::write(&marker, "").unwrap();
        assert_eq!(wait_for_file(&status), "[true,false,0,true,0]");
        std::fs::remove_file(status).unwrap();
        std::fs::remove_file(marker).unwrap();
    }
}

#[test]
fn an_output_handler_exception_escapes_an_early_finish() {
    let (root, server) = serve(&format!(
        r#"{WAIT_FOR_CLIENT}
ob_start(fn ($body) => "outer[$body]");
ob_start(fn () => throw new RuntimeException('handler'));
echo 'response';
try {{
    fastcgi_finish_request();
}} catch (RuntimeException $error) {{
    wait_for_client();
    $report = [$error->getMessage(), ob_get_level(), headers_sent(), $error->getTrace()[1]['function']];
    file_put_contents(__DIR__ . '/status', json_encode($report));
}}
echo 'late output';
"#
    ));
    let response = server.get_one("/");
    assert_eq!(response.split_once("\r\n\r\n").unwrap().1, "response");
    std::fs::write(root.path().join("continue"), "").unwrap();
    assert_eq!(
        wait_for_file(&root.path().join("status")),
        r#"["handler",0,true,"fastcgi_finish_request"]"#
    );
}

#[test]
fn a_disconnected_client_is_observed_at_the_next_transport_write() {
    for (streaming, flush_after_disconnect, ignore) in [
        (true, false, false),
        (true, false, true),
        (false, false, false),
        (false, true, false),
    ] {
        let (root, server) = serve(&format!(
            r#"{WAIT_FOR_CLIENT}
ignore_user_abort({ignore});
class ConnectionProbe {{
    function __destruct() {{
        file_put_contents(__DIR__ . '/destructor', (string) connection_status());
    }}
}}
$probe = new ConnectionProbe();
register_shutdown_function(function () {{
    file_put_contents(__DIR__ . '/status', (string) connection_status());
}});
if ({streaming}) {{
    echo 'first';
    flush();
}}
file_put_contents(__DIR__ . '/waiting', 'ready');
wait_for_client();
usleep(150000);
file_put_contents(__DIR__ . '/before', (string) connection_status());
echo 'after the client left';
if ({flush_after_disconnect}) flush();
file_put_contents(__DIR__ . '/after', (string) connection_aborted());
"#
        ));
        let mut client = start_request(&server, "/");
        if streaming {
            assert!(head(&mut client).contains("transfer-encoding: chunked"));
            assert_eq!(chunk(&mut client), b"first");
        }
        assert_eq!(wait_for_file(&root.path().join("waiting")), "ready");
        client.get_ref().shutdown(Shutdown::Both).unwrap();
        drop(client);
        std::fs::write(root.path().join("continue"), "").unwrap();
        let aborted = streaming || flush_after_disconnect;
        let status = if aborted { "1" } else { "0" };
        assert_eq!(wait_for_file(&root.path().join("status")), status);
        assert_eq!(wait_for_file(&root.path().join("destructor")), status);
        assert_eq!(wait_for_file(&root.path().join("before")), "0");
        let after = root.path().join("after");
        if ignore || !aborted {
            assert_eq!(wait_for_file(&after), status);
        } else {
            assert!(!after.exists());
        }
    }
}

fn wait_for_file(path: &Path) -> String {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(text) = std::fs::read_to_string(path)
            && !text.is_empty()
        {
            return text;
        }
        assert!(Instant::now() < until, "{} was not written", path.display());
        std::thread::sleep(Duration::from_millis(10));
    }
}
