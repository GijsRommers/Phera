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
async fn an_open_file_refuses_the_snapshot_and_names_where_it_is() {
    let error = boot_and_answer(
        "refusal",
        "<?php class Log { public static $file; } Log::$file = fopen(__FILE__, 'r'); Phera\\snapshot();",
        &[],
    )
    .await
    .expect_err("an open file cannot be kept");
    assert!(
        error.contains("Phera\\snapshot(): cannot keep stream (Log::$file)"),
        "{error}"
    );
    assert!(
        error.contains("an open file belongs to one request"),
        "{error}"
    );
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
