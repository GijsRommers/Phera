//! Five hundred 300 ms BLPOP waits overlap through `phera serve --workers 1`.
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinSet;

struct Server {
    process: Child,
    address: String,
    root: PathBuf,
}
impl Server {
    fn start(binary: &str) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../../target/redis-serve-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let script = format!(
            r#"<?php
$r=new Redis(); $r->connect('127.0.0.1',16399); $r->select(13);
$key='phera:concurrency:{}:'.$_SERVER['REQUEST_URI'];
$result=$r->blPop([$key],0.3);
if ($result === []) {{ echo 'done'; }} else {{ echo 'bad'; }}
"#,
            std::process::id()
        );
        std::fs::write(root.join("index.php"), script).unwrap();
        let mut process = Command::new(binary)
            .args(["serve", "--listen", "127.0.0.1:0", "--workers", "1"])
            .arg(&root)
            .env("PHERA_REDIS_POOL_SIZE", "512")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut banner = String::new();
        BufReader::new(process.stdout.take().unwrap())
            .read_line(&mut banner)
            .unwrap();
        let address = banner
            .split("http://")
            .nth(1)
            .and_then(|s| s.split_whitespace().next())
            .expect("server listen banner")
            .to_owned();
        Self {
            process,
            address,
            root,
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn five_hundred_waits_overlap() {
    let Ok(binary) = std::env::var("PHERA_BIN") else {
        eprintln!("Concurrency proof requires PHERA_BIN (set by CI)");
        return;
    };
    let server = Server::start(&binary);
    let started = Instant::now();
    let mut requests = JoinSet::new();
    for index in 0..500 {
        let address = server.address.clone();
        requests.spawn(async move {
            let mut stream = TcpStream::connect(address).await.unwrap();
            let request = format!(
                "GET /?key={index} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(request.as_bytes()).await.unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).await.unwrap();
            response
        });
    }
    let responses = tokio::time::timeout(Duration::from_secs(10), async {
        let mut responses = Vec::new();
        while let Some(response) = requests.join_next().await {
            responses.push(response.unwrap());
        }
        responses
    })
    .await
    .expect("500 requests timed out");
    let elapsed = started.elapsed();
    assert_eq!(responses.len(), 500);
    for response in responses {
        assert!(
            response.starts_with("HTTP/1.1 200") && response.contains("done"),
            "{response}"
        );
    }
    eprintln!("500 requests, workers=1, BLPOP timeout=300ms, elapsed={elapsed:?}");
    assert!(
        elapsed < Duration::from_secs(2),
        "single worker took {elapsed:?}"
    );
}
