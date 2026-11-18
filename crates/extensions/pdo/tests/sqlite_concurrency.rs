//! SQLite under concurrent HTTP requests: 200 requests that write to one WAL
//! database through two workers all succeed, in PHP's built-in server and in
//! `phera serve`; and on one Phera worker a quick request is answered while a
//! slow query is still running on the database's own thread.

use std::path::Path;
use std::time::{Duration, Instant};

use phera_extension_api::testing::oracle::{find_php, run_php};
use phera_extension_api::testing::serve::Server;

/// The body of a response, after its headers.
fn body(response: &str) -> &str {
    response.split_once("\r\n\r\n").map_or("", |(_, body)| body)
}

fn wal_database(directory: &Path) -> String {
    std::fs::create_dir_all(directory).unwrap();
    let database = directory
        .join("shared.db")
        .display()
        .to_string()
        .replace('\'', "''");
    run_php(
        "sqlite-wal-init",
        &format!(
            "<?php $p=new PDO('sqlite:{database}');\
             $p->exec('PRAGMA journal_mode=WAL; CREATE TABLE a(id INTEGER PRIMARY KEY, v)');"
        ),
    )
    .expect("PHP 8.5 oracle");
    std::fs::write(
        directory.join("index.php"),
        format!(
            "<?php $p=new PDO('sqlite:{database}');$p->setAttribute(PDO::ATTR_TIMEOUT,10);\
             $p->exec('INSERT INTO a(v) VALUES(42)');\
             $s=$p->prepare('SELECT v FROM a WHERE id=?');\
             $s->execute([$p->lastInsertId()]);echo $s->fetchColumn();"
        ),
    )
    .unwrap();
    database
}

#[test]
fn sqlite_wal_concurrency_and_worker_responsiveness() {
    let Some(phera) = std::env::var_os("PHERA_BIN") else {
        eprintln!("SKIP: set PHERA_BIN for SQLite HTTP concurrency");
        return;
    };
    let php = find_php().expect("PHP 8.5 oracle");
    let root = std::env::temp_dir().join(format!("phera-sqlite-http-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for runtime in ["php", "phera"] {
        let directory = root.join(runtime);
        let database = wal_database(&directory);
        let server = match runtime {
            "php" => Server::start_php(&php, &directory, 2),
            _ => Server::start(phera.as_ref(), &directory, 2, &[]),
        };
        let responses = server.get_all((0..200).map(|_| "/index.php".to_owned()));
        for response in &responses {
            assert_eq!(body(response), "42", "{runtime}: {response}");
        }
        let rows = run_php(
            "sqlite-wal-count",
            &format!("<?php echo (new PDO('sqlite:{database}'))->query('SELECT count(*) FROM a')->fetchColumn();"),
        )
        .unwrap();
        assert_eq!(rows, ["200"], "{runtime}: every request wrote its row");
    }

    std::fs::write(
        root.join("slow.php"),
        "<?php $p=new PDO('sqlite::memory:');$p->query('WITH RECURSIVE n(x) AS (VALUES(0) \
         UNION ALL SELECT x+1 FROM n WHERE x<5000000) SELECT sum(x) FROM n');echo 'slow';",
    )
    .unwrap();
    std::fs::write(
        root.join("quick.php"),
        "<?php $p=new PDO('sqlite::memory:');echo $p->query('SELECT 42')->fetchColumn();",
    )
    .unwrap();
    let server = Server::start(phera.as_ref(), &root, 1, &[]);
    assert_eq!(body(&server.get_one("/quick.php")), "42");
    std::thread::scope(|scope| {
        let slow = scope.spawn(|| server.get_one("/slow.php"));
        std::thread::sleep(Duration::from_millis(100));
        let started = Instant::now();
        assert_eq!(body(&server.get_one("/quick.php")), "42");
        let quick = started.elapsed();
        assert!(
            !slow.is_finished(),
            "the quick request waited for the slow query"
        );
        assert!(
            quick < Duration::from_millis(500),
            "quick request took {quick:?}"
        );
        assert_eq!(body(&slow.join().unwrap()), "slow");
        eprintln!("one worker: quick request in {quick:?} while the slow query ran");
    });
    drop(server);
    let _ = std::fs::remove_dir_all(root);
}
