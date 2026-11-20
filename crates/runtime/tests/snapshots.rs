//! Requests forked from a snapshot cannot see each other.
//!
//! `snapshots/isolation.php` builds state of every kind (objects, references,
//! statics, static variables, constants holding objects, closures, SPL
//! structures, WeakMap, ini settings, handlers), calls `Phera\snapshot()`,
//! and then changes all of it in the requests that ask for that. PHP-FPM
//! runs every request from the top, so each fork must print exactly what
//! PHP prints for one run of the file: the oracle is real PHP.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;

use phera_runtime::{BootReport, HttpRequest, Runtime, WebWorker};

mod snapshots {
    mod streams;
}

/// A directory of PHP files that is removed when the test ends.
struct Site {
    root: PathBuf,
}

impl Site {
    fn new(name: &str) -> Site {
        let root =
            std::env::temp_dir().join(format!("phera-snapshot-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create site directory");
        Site {
            root: root.canonicalize().expect("canonical site directory"),
        }
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

const ISOLATION: &str = include_str!("snapshots/isolation.php");

fn request(target: &str) -> HttpRequest {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let address: SocketAddr = "127.0.0.1:8080".parse().expect("address");
    HttpRequest {
        method: "GET".to_string(),
        target: target.to_string(),
        path: path.to_string(),
        query: query.to_string(),
        protocol: "HTTP/1.1".to_string(),
        headers: vec![("Host".to_string(), b"example.test".to_vec())],
        body: Vec::new(),
        remote_addr: "203.0.113.9:51000".parse().expect("address"),
        local_addr: address,
        https: false,
    }
}

/// What PHP prints for one run of the script, mutating or not.
fn php_output(script: &PathBuf, mutate: bool) -> Option<String> {
    let php = phera_extension_api::testing::oracle::find_php()?;
    let mut command = Command::new(php);
    command.arg("-n").arg(script);
    if mutate {
        command.env("MUTATE", "1");
    }
    let output = command.output().ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn answer(worker: &WebWorker, root: &Path, script: &Path, mutate: bool) -> String {
    let target = if mutate {
        "/index.php?mutate=1"
    } else {
        "/index.php"
    };
    let response = worker.handle_request(script, root, request(target)).await;
    String::from_utf8_lossy(&response.body).into_owned()
}

#[tokio::test(flavor = "current_thread")]
async fn forks_share_nothing_with_each_other_or_the_snapshot() {
    let site = Site::new("isolation");
    let script = site.root.join("index.php");
    std::fs::write(&script, ISOLATION).expect("write script");
    let (Some(plain), Some(mutated)) = (php_output(&script, false), php_output(&script, true))
    else {
        eprintln!("skipped: no PHP to compare with");
        return;
    };
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let worker = Runtime::new().web_worker(0);
            let report = worker.boot(&script, &site.root).await;
            assert!(
                matches!(report, BootReport::Snapshot { .. }),
                "the boot keeps a snapshot: {}",
                match report {
                    BootReport::Failed(message) => message,
                    _ => "it ended without one".into(),
                }
            );
            // One after the other: a change never reaches the next fork.
            for mutate in [false, true, false, true, true, false] {
                let expected = if mutate { &mutated } else { &plain };
                assert_eq!(
                    &answer(&worker, &site.root, &script, mutate).await,
                    expected
                );
            }
            // At the same time on one worker: each request sleeps in the
            // middle, so the others run between its change and its report.
            let pattern = [true, false, true, false, false, true, false, true];
            let tasks: Vec<_> = pattern
                .iter()
                .map(|&mutate| {
                    let (worker, root, script) =
                        (worker.clone(), site.root.clone(), script.clone());
                    tokio::task::spawn_local(async move {
                        (mutate, answer(&worker, &root, &script, mutate).await)
                    })
                })
                .collect();
            for task in tasks {
                let (mutate, answer) = task.await.expect("request task");
                let expected = if mutate { &mutated } else { &plain };
                assert_eq!(&answer, expected);
            }
            // And a fork taken after all of that still starts from the boot.
            assert_eq!(&answer(&worker, &site.root, &script, false).await, &plain);
        })
        .await;
}

/// Boots `source` as a site's index.php and answers `targets` from forks
/// of it, or reports why it did not keep a snapshot.
async fn boot_and_answer(
    name: &str,
    source: &str,
    targets: &[&str],
) -> Result<Vec<String>, String> {
    let site = Site::new(name);
    let script = site.root.join("index.php");
    std::fs::write(&script, source).expect("write script");
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let worker = Runtime::new().web_worker(0);
            match worker.boot(&script, &site.root).await {
                BootReport::Snapshot { .. } => {}
                BootReport::Classic => return Err("classic".into()),
                BootReport::Failed(message) => return Err(message),
            }
            let mut answers = Vec::new();
            for target in targets {
                let response = worker
                    .handle_request(&script, &site.root, request(target))
                    .await;
                answers.push(String::from_utf8_lossy(&response.body).into_owned());
            }
            Ok(answers)
        })
        .await
}

/// A file the boot opened is opened again in each fork, at the boot's
/// position: one fork's reads and writes do not move another's.
#[tokio::test(flavor = "current_thread")]
async fn an_open_file_of_the_boot_is_each_forks_own() {
    let answers = boot_and_answer(
        "file",
        "<?php file_put_contents(__DIR__ . '/data.txt', 'abcdef');
         $in = fopen(__DIR__ . '/data.txt', 'r'); fread($in, 2);
         $log = fopen(__DIR__ . '/log.txt', 'a');
         Phera\\snapshot();
         echo fread($in, 2), ' ', ftell($in);
         fwrite($log, 'x');
         clearstatcache(); echo ' ', filesize(__DIR__ . '/log.txt');",
        &["/", "/"],
    )
    .await
    .expect("snapshot");
    assert_eq!(answers, ["cd 4 1", "cd 4 2"]);
}

#[tokio::test(flavor = "current_thread")]
async fn a_locked_file_refuses_the_snapshot_and_names_where_it_is() {
    let error = boot_and_answer(
        "refusal",
        "<?php class Log { public static $file; } Log::$file = fopen(__FILE__, 'r');
         flock(Log::$file, LOCK_SH); Phera\\snapshot();",
        &[],
    )
    .await
    .expect_err("a locked file cannot be kept");
    assert!(
        error.contains("Phera\\snapshot(): cannot keep stream (Log::$file)"),
        "{error}"
    );
    assert!(error.contains("flock()"), "{error}");
}

#[tokio::test(flavor = "current_thread")]
async fn xml_snapshots_copy_memory_and_keep_simplexml_views_together() {
    let plain = r#"["boot","boot","item","pending","<r><item>boot<\/item><\/r>",["R","ITEM"],"boot resolver",false,false,false]"#;
    let changed = r#"["changed","changed","item","pending","<r><item>changed<\/item><\/r>",["R","ITEM"],"boot resolver",false,false,true]"#;
    let responses = boot_and_answer(
        "xml-family",
        include_str!("snapshots/xml_family.php"),
        &["/index.php", "/index.php?mutate=1", "/index.php"],
    )
    .await
    .expect("an XML snapshot made of plain data");
    assert_eq!(responses, [plain, changed, plain]);
}

#[tokio::test(flavor = "current_thread")]
async fn xml_snapshots_refuse_open_io() {
    for (name, script, class) in [
        (
            "xmlreader-stream",
            r#"<?php
                $stream = fopen('php://memory', 'r+');
                $reader = XMLReader::fromStream($stream);
                unset($stream);
                Phera\snapshot();
            "#,
            "XMLReader",
        ),
        (
            "xmlwriter-uri",
            r#"<?php
                $writer = XMLWriter::toUri(__DIR__ . '/output.xml');
                Phera\snapshot();
            "#,
            "XMLWriter",
        ),
    ] {
        let error = boot_and_answer(name, script, &[])
            .await
            .expect_err("an open XML handle");
        assert!(error.contains(&format!("cannot keep {class}")), "{error}");
        assert!(error.contains("belongs to one request"), "{error}");
    }
}

#[tokio::test(flavor = "current_thread")]
async fn a_script_without_the_call_is_classic() {
    let result = boot_and_answer("classic", "<?php echo 'no snapshot';", &[]).await;
    assert_eq!(result, Err("classic".to_string()));
}

#[tokio::test(flavor = "current_thread")]
async fn the_call_answers_true_in_every_request_and_nothing_before_it_is_sent() {
    let answers = boot_and_answer(
        "answers",
        "<?php echo 'boot output'; $kept = $_GET; var_dump(Phera\\snapshot(), $kept, $_GET['n'] ?? null, Phera\\snapshot());",
        &["/index.php?n=1", "/index.php?n=2"],
    )
    .await
    .expect("snapshot");
    assert_eq!(
        answers[0],
        "bool(true)\narray(0) {\n}\nstring(1) \"1\"\nbool(true)\n"
    );
    assert_eq!(
        answers[1],
        "bool(true)\narray(0) {\n}\nstring(1) \"2\"\nbool(true)\n"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn the_call_may_come_from_inside_functions_and_methods() {
    let answers = boot_and_answer(
        "depth",
        "<?php class App { function run($depth) { return $depth ? $this->run($depth - 1) : Phera\\snapshot(); } }
         function main() { $result = (new App)->run(3); echo $result ? 'resumed' : 'cli'; }
         main();",
        &["/index.php"],
    )
    .await
    .expect("snapshot");
    assert_eq!(answers, ["resumed"]);
}

#[tokio::test(flavor = "current_thread")]
async fn the_call_is_refused_inside_a_callback_of_a_built_in_function() {
    let error = boot_and_answer(
        "callback",
        "<?php array_map(fn () => Phera\\snapshot(), [1]);",
        &[],
    )
    .await
    .expect_err("refused");
    assert!(
        error.contains("cannot be called inside a callback of a built-in function"),
        "{error}"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn what_the_boot_wrote_into_server_is_there_in_every_request() {
    let answers = boot_and_answer(
        "server",
        "<?php $_SERVER['APP_ENV'] = 'prod'; unset($_SERVER['SERVER_SOFTWARE']); Phera\\snapshot();
         echo $_SERVER['APP_ENV'], ' ', isset($_SERVER['SERVER_SOFTWARE']) ? 'set' : 'unset', ' ', $_SERVER['REQUEST_URI'];",
        &["/index.php?x=1"],
    )
    .await
    .expect("snapshot");
    assert_eq!(answers, ["prod unset /index.php?x=1"]);
}

/// WordPress's `wp_fix_server_vars()` fills in `REQUEST_URI` when the
/// boot request has none; each request keeps its own.
#[tokio::test(flavor = "current_thread")]
async fn a_key_the_boot_added_yields_to_the_requests_own_value() {
    let answers = boot_and_answer(
        "added",
        "<?php if (empty($_SERVER['REQUEST_URI'])) { $_SERVER['REQUEST_URI'] = '/boot'; }
         $_SERVER['PHERA_ADDED'] = 'boot'; Phera\\snapshot();
         echo $_SERVER['REQUEST_URI'], ' ', $_SERVER['PHERA_ADDED'];",
        &["/one?a=1", "/two"],
    )
    .await
    .expect("snapshot");
    assert_eq!(answers, ["/one?a=1 boot", "/two boot"]);
}

#[tokio::test(flavor = "current_thread")]
async fn each_fork_destroys_its_own_copies_and_the_snapshot_keeps_its_objects() {
    let answers = boot_and_answer(
        "destructors",
        "<?php class Connection { function __destruct() { echo \"closed\\n\"; } }
         $connection = new Connection; Phera\\snapshot(); echo \"request\\n\";",
        &["/index.php", "/index.php"],
    )
    .await
    .expect("snapshot");
    assert_eq!(answers, ["request\nclosed\n", "request\nclosed\n"]);
}

#[tokio::test(flavor = "current_thread")]
async fn a_fork_keeps_independent_images_and_fileinfo_flags() {
    let answers = boot_and_answer(
        "gd-fileinfo",
        include_str!("snapshots/extension_state.php"),
        &[
            "/index.php?fork_probe=1",
            "/index.php?fork_probe=1&mutate=1",
            "/index.php?fork_probe=1",
        ],
    )
    .await
    .expect("GD and fileinfo boot state supports a snapshot");
    assert_eq!(
        answers,
        [
            "40203040 0 text/plain\n",
            "20224466 1 us-ascii\n",
            "40203040 0 text/plain\n",
        ]
    );
}

#[test]
fn on_the_command_line_the_call_returns_false_and_keeps_nothing() {
    let site = Site::new("cli");
    let script = site.root.join("cli.php");
    std::fs::write(&script, "<?php var_dump(Phera\\snapshot()); echo 'after';").expect("write");
    let result = Runtime::new()
        .run_file_blocking(&script, &[], None)
        .expect("run");
    assert_eq!(
        String::from_utf8_lossy(&result.output),
        "bool(false)\nafter"
    );
}

/// A PDO connection the boot made reconnects in each fork, with what the
/// boot changed about its session, and a fork's own change stays its own.
#[tokio::test(flavor = "current_thread")]
async fn a_pdo_connection_of_the_boot_reconnects_in_each_fork() {
    let answers = boot_and_answer(
        "pdo-sqlite",
        "<?php $pdo = new PDO('sqlite:' . __DIR__ . '/data.sqlite');
         $pdo->exec('CREATE TABLE IF NOT EXISTS t (id INTEGER)');
         $pdo->exec('PRAGMA cache_size = 1234');
         Phera\\snapshot();
         echo $pdo->query('PRAGMA cache_size')->fetchColumn();
         if (isset($_GET['change'])) { $pdo->exec('PRAGMA cache_size = 99'); }
         echo ' ', $pdo->query('SELECT COUNT(*) FROM t')->fetchColumn();",
        &["/?change=1", "/"],
    )
    .await
    .expect("snapshot");
    assert_eq!(answers, ["1234 0", "1234 0"]);
}

#[tokio::test(flavor = "current_thread")]
async fn an_in_memory_sqlite_database_refuses_the_snapshot() {
    let error = boot_and_answer(
        "pdo-memory",
        "<?php $pdo = new PDO('sqlite::memory:'); Phera\\snapshot();",
        &[],
    )
    .await
    .expect_err("refused");
    assert!(error.contains("in-memory SQLite database"), "{error}");
}

#[tokio::test(flavor = "current_thread")]
async fn an_open_transaction_refuses_the_snapshot() {
    let error = boot_and_answer(
        "pdo-transaction",
        "<?php $pdo = new PDO('sqlite:' . __DIR__ . '/data.sqlite');
         $pdo->beginTransaction(); Phera\\snapshot();",
        &[],
    )
    .await
    .expect_err("refused");
    assert!(error.contains("a transaction is open"), "{error}");
}

/// Whether the test MySQL of `crates/extensions/pdo/README.md` answers.
fn mysql_available() -> bool {
    let available = std::net::TcpStream::connect_timeout(
        &"127.0.0.1:33069".parse().expect("address"),
        std::time::Duration::from_millis(300),
    )
    .is_ok();
    if !available {
        eprintln!("SKIP: the test MySQL on port 33069 is not running");
    }
    available
}

/// mysqli and PDO connections of the boot: each fork has its own session,
/// with the boot's settings replayed, and what one fork sets is gone in
/// the next.
#[tokio::test(flavor = "current_thread")]
async fn mysql_connections_of_the_boot_get_a_session_of_their_own_in_each_fork() {
    if !mysql_available() {
        return;
    }
    let answers = boot_and_answer(
        "mysql",
        "<?php $db = new mysqli('127.0.0.1', 'phera', 'phera', 'phera_test', 33069);
         $db->set_charset('latin1');
         $db->query(\"SET @boot = 'mysqli'\");
         $pdo = new PDO('mysql:host=127.0.0.1;port=33069;dbname=phera_test', 'phera', 'phera');
         $pdo->exec(\"SET @boot = 'pdo'\");
         $stored = $db->query('SELECT 1 UNION SELECT 2');
         $stored->fetch_row();
         Phera\\snapshot();
         $row = $db->query('SELECT @boot, @fork, @@character_set_client')->fetch_row();
         echo implode(',', array_map('strval', $row)), ' ';
         echo implode(',', array_map('strval', $pdo->query('SELECT @boot, @fork')->fetch(PDO::FETCH_NUM))), ' ';
         echo $stored->fetch_row()[0];
         $db->query(\"SET @fork = 'set'\");
         $pdo->exec(\"SET @fork = 'set'\");",
        &["/", "/"],
    )
    .await
    .expect("snapshot");
    assert_eq!(answers, ["mysqli,,latin1 pdo, 2", "mysqli,,latin1 pdo, 2"]);
}

/// A pgsql connection of the boot: each fork borrows its own session with
/// the boot's `SET` replayed; what one fork sets is gone in the next.
#[tokio::test(flavor = "current_thread")]
async fn a_pgsql_connection_of_the_boot_gets_a_session_of_its_own_in_each_fork() {
    let available = std::net::TcpStream::connect_timeout(
        &"127.0.0.1:54329".parse().expect("address"),
        std::time::Duration::from_millis(300),
    )
    .is_ok();
    if !available {
        eprintln!("SKIP: the test PostgreSQL on port 54329 is not running");
        return;
    }
    let answers = boot_and_answer(
        "pgsql",
        "<?php $db = pg_connect('host=127.0.0.1 port=54329 dbname=phera_test user=phera password=phera');
         pg_query($db, \"SET application_name = 'boot'\");
         Phera\\snapshot();
         echo pg_fetch_result(pg_query($db, 'SHOW application_name'), 0, 0), ' ';
         echo pg_fetch_result(pg_query($db, \"SELECT current_setting('phera.fork', true)\"), 0, 0) ?? 'unset';
         pg_query($db, \"SET phera.fork = 'set'\");",
        &["/", "/"],
    )
    .await
    .expect("snapshot");
    assert_eq!(answers, ["boot unset", "boot unset"]);
}

/// A Redis connection of the boot: each fork borrows its own socket, in the
/// database the boot selected.
#[tokio::test(flavor = "current_thread")]
async fn a_redis_connection_of_the_boot_gets_a_socket_of_its_own_in_each_fork() {
    let available = std::net::TcpStream::connect_timeout(
        &"127.0.0.1:16399".parse().expect("address"),
        std::time::Duration::from_millis(300),
    )
    .is_ok();
    if !available {
        eprintln!("SKIP: the test Redis on port 16399 is not running");
        return;
    }
    let answers = boot_and_answer(
        "redis",
        "<?php $redis = new Redis(); $redis->connect('127.0.0.1', 16399); $redis->select(14);
         $redis->set('phera:snapshot', 'boot');
         Phera\\snapshot();
         echo $redis->get('phera:snapshot'), ' ', $redis->getDbNum();
         $redis->select(15);",
        &["/", "/"],
    )
    .await
    .expect("snapshot");
    assert_eq!(answers, ["boot 14", "boot 14"]);
}

/// A DOM document of the boot: each fork's nodes are its own, one copy of
/// the document per fork shared by all of them, so a change through one
/// node object is seen through another, in that fork only.
#[tokio::test(flavor = "current_thread")]
async fn a_dom_document_of_the_boot_is_each_forks_own() {
    let answers = boot_and_answer(
        "dom",
        "<?php $d = new DOMDocument(); $d->loadXML('<a><b>x</b></a>');
         $b = $d->getElementsByTagName('b')->item(0);
         $hash = hash_init('sha256'); hash_update($hash, 'boot');
         $curl = curl_init('http://127.0.0.1:1/'); curl_setopt($curl, CURLOPT_PRIVATE, 'boot');
         Phera\\snapshot();
         echo $d->saveXML($d->documentElement), ' ', $b === $d->documentElement->firstChild ? 'same' : 'other';
         $b->nodeValue = 'changed'; $d->documentElement->setAttribute('fork', '1');
         echo ' ', $d->saveXML($d->documentElement);
         echo ' ', substr(hash_final($hash), 0, 8), ' ', curl_getinfo($curl, CURLINFO_PRIVATE);
         curl_setopt($curl, CURLOPT_PRIVATE, 'fork');",
        &["/", "/"],
    )
    .await
    .expect("snapshot");
    let expected = "<a><b>x</b></a> same <a fork=\"1\"><b>changed</b></a> 4509beb0 boot";
    assert_eq!(answers, [expected, expected]);
}

/// Generators and fibers of the boot: each fork resumes its own copy from
/// where the boot left it, and a suspended generator's `finally` runs at
/// the end of every request, as it would in a run from the top.
#[tokio::test(flavor = "current_thread")]
async fn generators_and_fibers_of_the_boot_are_each_forks_own() {
    let answers = boot_and_answer(
        "generators",
        "<?php function count_up() { try { $i = 0; while (true) { $got = yield $i++; if ($got) echo \"[$got]\"; } } finally { echo '(closed)'; } }
         $counter = count_up(); $counter->current(); $counter->next();
         function done() { yield 1; } $done = done(); foreach ($done as $_) {}
         $fiber = new Fiber(fn (int $n) => $n * 2);
         $finished = new Fiber(fn () => 'ok'); $finished->start();
         Phera\\snapshot();
         echo $counter->current(), $counter->send('x'), ' ';
         echo $done->valid() ? 'valid' : 'done', ' ';
         $fiber->start(21); echo $fiber->getReturn(), ' ', $finished->getReturn(), ' ';",
        &["/", "/"],
    )
    .await
    .expect("snapshot");
    assert_eq!(
        answers,
        ["1[x]2 done 42 ok (closed)", "1[x]2 done 42 ok (closed)"]
    );
}

/// Temporary files, unused compression contexts and curl's multi and share
/// handles of the boot: each fork has its own.
#[tokio::test(flavor = "current_thread")]
async fn temporary_files_and_unused_contexts_of_the_boot_are_each_forks_own() {
    let answers = boot_and_answer(
        "temporary",
        "<?php $temp = fopen('php://temp/maxmemory:4', 'w+'); fwrite($temp, 'boot-data');
         $file = tmpfile(); fwrite($file, 'tmp');
         $deflate = deflate_init(ZLIB_ENCODING_DEFLATE);
         $multi = curl_multi_init(); $share = curl_share_init();
         Phera\\snapshot();
         fwrite($temp, '+fork'); rewind($temp); echo stream_get_contents($temp), ' ';
         fwrite($file, '+fork'); rewind($file); echo stream_get_contents($file), ' ';
         echo bin2hex(gzuncompress(deflate_add($deflate, 'abc', ZLIB_FINISH))), ' ';
         echo get_class($multi), ' ', get_class($share);",
        &["/", "/"],
    )
    .await
    .expect("snapshot");
    let expected = "boot-data+fork tmp+fork 616263 CurlMultiHandle CurlShareHandle";
    assert_eq!(answers, [expected, expected]);
}

/// Two files that declare a function of the same name, run by different
/// requests of one worker: each request calls its own, although the worker
/// numbers both files' functions in one table.
#[tokio::test(flavor = "current_thread")]
async fn two_files_with_the_same_function_name_on_different_requests_of_a_worker() {
    let site = Site::new("same-function");
    std::fs::write(
        site.root.join("a.php"),
        "<?php function helper() { return 'a'; } echo helper();",
    )
    .expect("a.php");
    std::fs::write(site.root.join("b.php"), "<?php function helper() { return 'b'; } echo helper(), ' ', function_exists('other') ? 'other' : 'none';").expect("b.php");
    std::fs::write(
        site.root.join("c.php"),
        "<?php function other() {} echo function_exists('helper') ? 'helper' : 'none';",
    )
    .expect("c.php");
    let local = tokio::task::LocalSet::new();
    let answers = local
        .run_until(async {
            let worker = Runtime::new().web_worker(0);
            let mut answers = Vec::new();
            for script in ["a.php", "b.php", "c.php", "a.php", "b.php"] {
                let path = site.root.join(script);
                let response = worker
                    .handle_request(&path, &site.root, request(&format!("/{script}")))
                    .await;
                answers.push(String::from_utf8_lossy(&response.body).into_owned());
            }
            answers
        })
        .await;
    assert_eq!(answers, ["a", "b none", "none", "a", "b none"]);
}

/// Runs `targets` (script and query) as requests of one unbooted worker,
/// whose requests share the worker's linked classes.
async fn worker_answers(runtime: Runtime, site: &Site, targets: &[&str]) -> Vec<String> {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let worker = runtime.web_worker(0);
            let mut answers = Vec::new();
            for target in targets {
                let script = site.root.join(target.split('?').next().unwrap_or(target));
                let response = worker
                    .handle_request(&script, &site.root, request(&format!("/{target}")))
                    .await;
                answers.push(String::from_utf8_lossy(&response.body).into_owned());
            }
            answers
        })
        .await
}

/// A class file edited between two requests of a worker: the second request
/// links the new revision, as a fresh worker would; the old revision's
/// linked class is not reused.
#[tokio::test(flavor = "current_thread")]
async fn a_class_file_edited_between_two_requests_links_its_new_revision() {
    let site = Site::new("edited-class");
    let model = site.root.join("model.php");
    std::fs::write(
        &model,
        "<?php class Model { function value() { return 'old'; } }",
    )
    .expect("model");
    std::fs::write(site.root.join("index.php"), "<?php require __DIR__ . '/model.php'; $m = new Model; echo $m->value(), ' ', method_exists($m, 'added') ? 'added' : 'none';").expect("index");
    let runtime = Runtime::new().with_web_development(true);
    let local = tokio::task::LocalSet::new();
    let answers = local
        .run_until(async {
            let worker = runtime.web_worker(0);
            let index = site.root.join("index.php");
            let mut answers = Vec::new();
            for step in 0..4 {
                if step == 2 {
                    std::thread::sleep(std::time::Duration::from_millis(20));
                    std::fs::write(&model, "<?php class Model { function value() { return 'new'; } function added() {} }").expect("edit");
                }
                let response = worker.handle_request(&index, &site.root, request("/index.php")).await;
                answers.push(String::from_utf8_lossy(&response.body).into_owned());
            }
            answers
        })
        .await;
    assert_eq!(answers, ["old none", "old none", "new added", "new added"]);
}

/// Conditional declarations resolve per request: a class declared in either
/// branch of an `if`, and one name declared by two files on alternating
/// requests, are each request's own class.
#[tokio::test(flavor = "current_thread")]
async fn conditional_and_same_name_declarations_resolve_per_request() {
    let site = Site::new("conditional-classes");
    std::fs::write(site.root.join("branch.php"), "<?php if (isset($_GET['one'])) { class C { function n() { return 'one'; } } } else { class C { function n() { return 'two'; } } } echo (new C)->n();").expect("branch");
    std::fs::write(site.root.join("first.php"), "<?php class D { const V = 'first'; } echo D::V, ' ', class_exists('E', false) ? 'E' : 'no E';").expect("first");
    std::fs::write(site.root.join("second.php"), "<?php class D { const V = 'second'; } class E {} echo D::V, ' ', class_exists('E', false) ? 'E' : 'no E';").expect("second");
    let answers = worker_answers(
        Runtime::new(),
        &site,
        &[
            "branch.php?one=1",
            "branch.php",
            "branch.php?one=1",
            "first.php",
            "second.php",
            "first.php",
            "second.php",
        ],
    )
    .await;
    assert_eq!(
        answers,
        [
            "one",
            "two",
            "one",
            "first no E",
            "second E",
            "first no E",
            "second E"
        ]
    );
}

/// A class whose parent is a different class in two requests is linked by
/// each: its linked class from the first request is not reused.
#[tokio::test(flavor = "current_thread")]
async fn a_class_with_another_parent_in_another_request_is_linked_again() {
    let site = Site::new("other-parent");
    std::fs::write(site.root.join("child.php"), "<?php class Child extends Base { use Greets; function name() { return parent::name() . '+child'; } }").expect("child");
    std::fs::write(
        site.root.join("greets.php"),
        "<?php trait Greets { function greet() { return 'hi ' . $this->name(); } }",
    )
    .expect("trait");
    std::fs::write(site.root.join("a.php"), "<?php class Base { function name() { return 'a'; } } require __DIR__ . '/greets.php'; require __DIR__ . '/child.php'; echo (new Child)->greet();").expect("a");
    std::fs::write(site.root.join("b.php"), "<?php class Base { function name() { return 'b'; } } require __DIR__ . '/greets.php'; require __DIR__ . '/child.php'; echo (new Child)->greet();").expect("b");
    let answers =
        worker_answers(Runtime::new(), &site, &["a.php", "b.php", "a.php", "b.php"]).await;
    assert_eq!(
        answers,
        ["hi a+child", "hi b+child", "hi a+child", "hi b+child"]
    );
}

/// `class_alias` names are each request's own: an alias made in one request
/// of a worker is no class in the next, and may name another class there.
#[tokio::test(flavor = "current_thread")]
async fn class_aliases_are_each_requests_own() {
    let site = Site::new("aliases");
    std::fs::write(
        site.root.join("classes.php"),
        "<?php class Alpha { const N = 'alpha'; } class Beta { const N = 'beta'; }",
    )
    .expect("classes");
    std::fs::write(site.root.join("alias.php"), "<?php require __DIR__ . '/classes.php'; if (isset($_GET['to'])) { class_alias($_GET['to'], 'Named'); } echo class_exists('Named', false) ? Named::N : 'none', ' ', class_exists('stdAlias', false) ? 'std' : 'nostd'; if (isset($_GET['std'])) { class_alias('stdClass', 'stdAlias'); echo ' ', get_class(new stdAlias); }").expect("alias");
    let answers = worker_answers(
        Runtime::new(),
        &site,
        &[
            "alias.php?to=Alpha",
            "alias.php",
            "alias.php?to=Beta&std=1",
            "alias.php",
        ],
    )
    .await;
    assert_eq!(
        answers,
        [
            "alpha nostd",
            "none nostd",
            "beta nostd stdClass",
            "none nostd"
        ]
    );
}

/// Class names ignore case in static references (`new POST`), in strings a
/// script builds, and across requests that share the class.
#[tokio::test(flavor = "current_thread")]
async fn class_lookups_ignore_case_in_every_request() {
    let site = Site::new("case-insensitive");
    std::fs::write(
        site.root.join("post.php"),
        "<?php class Post { public static function query() { return 'query'; } }",
    )
    .expect("post");
    std::fs::write(site.root.join("index.php"), "<?php require __DIR__ . '/post.php'; $name = 'pOsT'; echo get_class(new POST), ' ', post::query(), ' ', get_class(new $name), ' ', class_exists('POST', false) ? 'yes' : 'no', ' ', (new Post) instanceof pOST ? 'is' : 'not';").expect("index");
    let answers = worker_answers(
        Runtime::new(),
        &site,
        &["index.php", "index.php", "index.php"],
    )
    .await;
    assert_eq!(answers, ["Post query Post yes is"; 3]);
}
