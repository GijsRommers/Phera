//! `phera serve --request-timeout`: a script that does not finish is
//! stopped, and the worker goes on serving.

use std::time::{Duration, Instant};

use super::super::Served;

#[test]
fn a_script_that_does_not_finish_is_stopped() {
    let served = Served::start_in(
        "request-timeout",
        1,
        &[
            ("sleep.php", "<?php usleep(20000000); echo 'slept';"),
            ("spin.php", "<?php $i = 0; while (true) { $i++; }"),
            (
                "write.php",
                "<?php $pdo = new PDO('sqlite::memory:'); \
                 $pdo->exec('CREATE TABLE t (n INTEGER)'); $pdo->beginTransaction(); \
                 for ($i = 0; ; $i++) { $pdo->exec(\"INSERT INTO t VALUES ($i)\"); }",
            ),
            ("quick.php", "<?php echo 'quick';"),
        ],
        ".",
        &["--request-timeout", "1"],
    );
    // One worker: if a stopped script kept running, the next request would
    // wait behind it.
    for script in ["/sleep.php", "/spin.php", "/write.php"] {
        let started = Instant::now();
        let (status, _) = served.get(script);
        assert_eq!(status, 504, "{script}");
        assert!(started.elapsed() < Duration::from_secs(5), "{script}");
        let started = Instant::now();
        assert_eq!(served.get("/quick.php"), (200, "quick".to_string()));
        assert!(started.elapsed() < Duration::from_secs(1), "after {script}");
    }
}

#[test]
fn the_receive_budget_is_shared_by_the_head_and_body() {
    use std::io::{Read, Write};
    use std::net::TcpStream;

    let served = Served::start_in(
        "receive-timeout",
        1,
        &[("index.php", "<?php echo 'complete';")],
        ".",
        &["--request-read-timeout", "1"],
    );
    let connect = || {
        let stream = TcpStream::connect(&served.address).expect("connect to server");
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("set timeout");
        stream
    };
    let mut head = connect();
    head.write_all(b"GET / HTTP/1.1\r\nHost: local")
        .expect("send partial head");
    let mut response = String::new();
    head.read_to_string(&mut response)
        .expect("read head timeout");
    assert!(response.starts_with("HTTP/1.1 408"), "{response}");

    let mut body = connect();
    let started = Instant::now();
    body.write_all(b"POST / HTTP/1.1\r\nHost: local\r\nContent-Length: 10\r\n")
        .expect("start request head");
    std::thread::sleep(Duration::from_millis(550));
    body.write_all(b"\r\na")
        .expect("finish head and start body");
    std::thread::sleep(Duration::from_millis(200));
    body.write_all(b"b").expect("send more body");
    response.clear();
    body.read_to_string(&mut response)
        .expect("read body timeout");
    assert!(response.starts_with("HTTP/1.1 408"), "{response}");
    assert!(
        started.elapsed() < Duration::from_millis(1450),
        "{:?}",
        started.elapsed()
    );
    let mut steady = connect();
    steady
        .write_all(
            b"POST / HTTP/1.1\r\nHost: local\r\nConnection: close\r\nContent-Length: 32769\r\n\r\n",
        )
        .expect("send headers for a large upload");
    steady
        .write_all(&[b'a'; 32768])
        .expect("send first part at speed");
    std::thread::sleep(Duration::from_millis(1200));
    steady
        .write_all(b"b")
        .expect("send last part using earned time");
    response.clear();
    steady
        .read_to_string(&mut response)
        .expect("read completed upload response");
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");

    assert_eq!(served.get("/"), (200, "complete".to_string()));
}
