//! Forty `SELECT pg_sleep(0.3)` requests must overlap on one Phera worker thread.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinSet;

mod support;

struct Server {
    process: Child,
    address: String,
    root: PathBuf,
}

impl Server {
    fn start(binary: &str, driver: &str) -> Self {
        let root = support::scratch().join(format!("serve-{driver}"));
        std::fs::create_dir_all(&root).unwrap();
        let query = match driver {
            "pgsql" => "SELECT pg_sleep(0.3)",
            _ => "SELECT SLEEP(0.3)",
        };
        let script = format!(
            "<?php $p=new PDO('{}','phera','phera');$p->query('{query}');echo 'done';",
            support::dsn(driver)
        );
        std::fs::write(root.join("index.php"), script).unwrap();
        let mut process = Command::new(binary)
            .args(["serve", "--listen", "127.0.0.1:0", "--workers", "1"])
            .arg(&root)
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
            .and_then(|rest| rest.split_whitespace().next())
            .expect("server listen banner");
        Self {
            process,
            address: address.to_owned(),
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
async fn forty_database_waits_overlap_on_one_worker() {
    let Ok(binary) = std::env::var("PHERA_BIN") else {
        eprintln!("SKIP: set PHERA_BIN for the single-worker concurrency proof");
        return;
    };
    if !support::databases_available() {
        return;
    }
    let mut failures = Vec::new();
    for driver in ["pgsql", "mysql"] {
        let server = Server::start(&binary, driver);
        let started = Instant::now();
        let mut requests = JoinSet::new();
        for _ in 0..40 {
            let address = server.address.clone();
            requests.spawn(async move {
                let mut stream = TcpStream::connect(address).await.unwrap();
                stream
                    .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .await
                    .unwrap();
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
        .expect("database requests timed out");
        let elapsed = started.elapsed();
        if let Some(response) = responses
            .iter()
            .find(|response| !response.starts_with("HTTP/1.1 200") || !response.contains("done"))
        {
            failures.push(format!("{driver}: {response}"));
            continue;
        }
        eprintln!("{driver}: 40 waits on one worker completed in {elapsed:?}");
        if elapsed >= Duration::from_secs(2) {
            failures.push(format!("{driver}: elapsed {elapsed:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
