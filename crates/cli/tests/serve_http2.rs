//! End-to-end HTTP/1.1 vs HTTP/2 (`h2c`) tests against the `phera serve`
//! binary.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use http_body_util::{BodyExt, Full};
use hyper::Request;
use hyper::body::Bytes;
use hyper::client::conn::{http1, http2};
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpStream;

struct Served {
    process: Child,
    address: String,
    root: PathBuf,
}

impl Served {
    fn start(name: &str, files: &[(&str, &str)], flags: &[&str]) -> Self {
        let root =
            std::env::temp_dir().join(format!("phera-serve-h2-{name}-{}", std::process::id()));
        for (path, contents) in files {
            let file = root.join(path);
            std::fs::create_dir_all(file.parent().expect("parent")).expect("create dir");
            std::fs::write(file, contents).expect("write file");
        }
        let mut process = Command::new(env!("CARGO_BIN_EXE_phera"))
            .args(["serve", "--listen", "127.0.0.1:0", "--workers", "2"])
            .current_dir(&root)
            .args(flags)
            .arg(&root)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("start phera serve");
        let stdout = process.stdout.take().expect("stdout");
        let mut banner = String::new();
        BufReader::new(stdout)
            .read_line(&mut banner)
            .expect("read banner");
        let address = banner
            .split("http://")
            .nth(1)
            .and_then(|rest| rest.split_whitespace().next())
            .unwrap_or_else(|| panic!("no address in banner: {banner:?}"))
            .to_string();
        Self {
            process,
            address,
            root,
        }
    }
}

impl Drop for Served {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

async fn request(
    addr: &str,
    h2: bool,
    method: &str,
    target: &str,
    headers: &[(&str, &str)],
    body: &'static [u8],
) -> (u16, String) {
    let stream = TcpStream::connect(addr).await.unwrap();
    let io = TokioIo::new(stream);
    let mut builder = if h2 {
        Request::builder()
            .method(method)
            .uri(format!("http://{addr}{target}"))
    } else {
        Request::builder()
            .method(method)
            .uri(target)
            .header("host", addr)
    };
    for (k, v) in headers {
        builder = builder.header(*k, *v);
    }
    let req = builder.body(Full::new(Bytes::from_static(body))).unwrap();

    let resp = if h2 {
        let (mut sender, conn) = http2::Builder::new(TokioExecutor::new())
            .handshake(io)
            .await
            .unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        sender.send_request(req).await.unwrap()
    } else {
        let (mut sender, conn) = http1::handshake(io).await.unwrap();
        tokio::spawn(async move {
            let _ = conn.await;
        });
        sender.send_request(req).await.unwrap()
    };

    let status = resp.status().as_u16();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

#[tokio::test(flavor = "current_thread")]
async fn php_and_static_requests_match_across_http1_and_http2() {
    let php = r#"<?php
header('Connection: close');
echo json_encode([
    'protocol' => $_SERVER['SERVER_PROTOCOL'],
    'method' => $_SERVER['REQUEST_METHOD'],
    'uri' => $_SERVER['REQUEST_URI'],
    'query' => $_SERVER['QUERY_STRING'] ?? '',
    'host' => $_SERVER['HTTP_HOST'] ?? '',
    'server_name' => $_SERVER['SERVER_NAME'] ?? '',
    'custom' => $_SERVER['HTTP_X_CUSTOM'] ?? '',
    'body' => file_get_contents('php://input'),
]);
"#;
    let served = Served::start(
        "parity",
        &[("index.php", php), ("hello.txt", "static over wire\n")],
        &["--health", "/healthz"],
    );

    let (h1_status, h1_body) = request(
        &served.address,
        false,
        "POST",
        "/index.php?a=1&b=2",
        &[("x-custom", "yes")],
        b"ping",
    )
    .await;
    let (h2_status, h2_body) = request(
        &served.address,
        true,
        "POST",
        "/index.php?a=1&b=2",
        &[("x-custom", "yes")],
        b"ping",
    )
    .await;

    assert_eq!((h1_status, h2_status), (200, 200));
    let h1_json: serde_json::Value = serde_json::from_str(&h1_body).unwrap();
    let mut h2_json: serde_json::Value = serde_json::from_str(&h2_body).unwrap();
    assert_eq!(h1_json["protocol"], "HTTP/1.1");
    assert_eq!(h2_json["protocol"], "HTTP/2.0");
    h2_json["protocol"] = serde_json::Value::String("HTTP/1.1".to_string());
    assert_eq!(h1_json, h2_json);

    let (s1, b1) = request(&served.address, false, "GET", "/hello.txt", &[], b"").await;
    let (s2, b2) = request(&served.address, true, "GET", "/hello.txt", &[], b"").await;
    assert_eq!((s1, b1.as_str()), (200, "static over wire\n"));
    assert_eq!((s2, b2.as_str()), (200, "static over wire\n"));

    let (hs, hb) = request(&served.address, true, "GET", "/healthz", &[], b"").await;
    assert_eq!((hs, hb.as_str()), (200, "ok\n"));
}
