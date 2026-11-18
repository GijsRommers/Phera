//! Forty requests that each wait 0.3 s for the database overlap on one
//! `phera serve` worker thread: together they take well under two seconds.

use std::time::{Duration, Instant};

use phera_extension_api::testing::serve::Server;

mod support;

#[test]
fn forty_database_waits_overlap_on_one_worker() {
    let Some(binary) = std::env::var_os("PHERA_BIN") else {
        eprintln!("SKIP: set PHERA_BIN for the single-worker concurrency proof");
        return;
    };
    if !support::databases_available() {
        return;
    }
    for driver in ["pgsql", "mysql"] {
        let root = std::env::temp_dir().join(format!("phera-pdo-serve-{driver}"));
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
        let server = Server::start(binary.as_ref(), &root, 1, &[]);
        let started = Instant::now();
        let responses = server.get_all((0..40).map(|_| "/".to_owned()));
        let elapsed = started.elapsed();
        for response in &responses {
            assert!(
                response.starts_with("HTTP/1.1 200") && response.contains("done"),
                "{driver}: {response}"
            );
        }
        eprintln!("{driver}: 40 waits on one worker completed in {elapsed:?}");
        assert!(
            elapsed < Duration::from_secs(2),
            "{driver}: elapsed {elapsed:?}"
        );
    }
}
