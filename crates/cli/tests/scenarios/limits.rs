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
