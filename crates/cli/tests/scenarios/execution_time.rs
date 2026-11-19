//! On one worker, a spinning PHP request can time out while another request runs.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::super::Served;

#[test]
fn a_php_timeout_leaves_the_worker_and_its_other_requests_running() {
    let served = Served::start_in(
        "php-execution-time",
        1,
        &[
            (
                "spin.php",
                r#"<?php
ob_start();
register_shutdown_function(fn () => print "shutdown\n");
file_put_contents(__DIR__ . '/spinning', 'yes');
set_time_limit(1);
while (true) {}
"#,
            ),
            (
                "quick.php",
                "<?php echo 'quick:', ini_get('max_execution_time');",
            ),
        ],
        ".",
        &["--request-timeout", "15"],
    );

    thread::scope(|scope| {
        let (send, slow) = mpsc::channel();
        let server = &served;
        scope.spawn(move || {
            send.send(server.get("/spin.php")).expect("send response");
        });
        let started = Instant::now();
        while !served.root.join("spinning").is_file() {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "PHP did not start"
            );
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(served.get("/quick.php"), (200, "quick:30".into()));
        assert!(matches!(slow.try_recv(), Err(mpsc::TryRecvError::Empty)));
        let (status, body) = slow
            .recv_timeout(Duration::from_secs(20))
            .expect("timed-out response");
        assert_eq!(status, 200);
        assert!(body.starts_with("\nFatal error: Maximum execution time of 1 second exceeded in "));
        assert!(body.ends_with("spin.php on line 6\nStack trace:\n#0 {main}\nshutdown\n"));
        assert_eq!(served.get("/quick.php"), (200, "quick:30".into()));
    });
}

#[test]
fn a_hidden_timeout_sets_an_http_error_status() {
    let served = Served::start_in(
        "hidden-php-execution-time",
        1,
        &[(
            "spin.php",
            "<?php register_shutdown_function(fn () => print http_response_code()); while (true) {}",
        )],
        ".",
        &[
            "--request-timeout",
            "15",
            "-d",
            "display_errors=0",
            "-d",
            "max_execution_time=1",
        ],
    );
    assert_eq!(served.get("/spin.php"), (500, "500".to_owned()));
}

#[test]
fn time_waiting_in_a_native_future_does_not_use_the_php_budget() {
    let served = Served::start_with_workers(
        "php-execution-time-wait",
        1,
        &[(
            "wait.php",
            "<?php set_time_limit(1); usleep(1250000); echo 'finished';",
        )],
    );
    assert_eq!(served.get("/wait.php"), (200, "finished".to_owned()));
}
