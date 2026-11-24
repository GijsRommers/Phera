//! Five hundred requests that each wait 300 ms in BLPOP overlap on one
//! `phera serve` worker thread: together they take well under two seconds.

use std::time::{Duration, Instant};

use phera_extension_api::testing::serve::Server;

#[test]
fn five_hundred_waits_overlap() {
    let Some(binary) = std::env::var_os("PHERA_BIN") else {
        eprintln!("SKIP: set PHERA_BIN for the single-worker concurrency proof");
        return;
    };
    let root = std::env::temp_dir().join(format!("phera-redis-serve-{}", std::process::id()));
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
    let server = Server::start(
        binary.as_ref(),
        &root,
        1,
        &[
            ("PHERA_REDIS_POOL_SIZE", "512"),
            ("PHERA_MAX_CONCURRENT_REQUESTS_PER_WORKER", "512"),
        ],
    );
    let started = Instant::now();
    let responses = server.get_all((0..500).map(|index| format!("/?key={index}")));
    let elapsed = started.elapsed();
    for response in &responses {
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
    drop(server);
    let _ = std::fs::remove_dir_all(root);
}
