//! A worker that has seen which classes its requests autoload boots again
//! and declares them in the snapshot, through the application's own
//! autoloaders, just before `Phera\snapshot()` keeps the state. After that,
//! requests find the classes declared and run no autoloader for them.
//!
//! The one stated difference from PHP (the same one `opcache.preload` has):
//! a warmed-up class exists before the request starts, so
//! `class_exists($name, false)` is true and the autoloader is not called.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use phera_runtime::{BootReport, HttpRequest, Runtime, WebWorker};

/// A directory of PHP files that is removed when the test ends.
struct Site {
    root: PathBuf,
}

impl Site {
    fn new(name: &str) -> Site {
        let root = std::env::temp_dir().join(format!("phera-warmup-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/Warm")).expect("create site directory");
        Site {
            root: root.canonicalize().expect("canonical site directory"),
        }
    }

    fn write(&self, name: &str, content: &str) {
        std::fs::write(self.root.join(name), content).expect("write file");
    }

    fn read(&self, name: &str) -> String {
        std::fs::read_to_string(self.root.join(name)).unwrap_or_default()
    }

    fn log_lines(&self) -> usize {
        self.read("autoload.log")
            .lines()
            .filter(|line| !line.is_empty())
            .count()
    }
}

impl Drop for Site {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// An entry script that registers an autoloader logging every call, takes
/// the snapshot, and then asks for `Warm\Thing` (or `Warm\Flaky`), catching
/// what a failing autoloader throws.
const INDEX: &str = r#"<?php
spl_autoload_register(function (string $class): void {
    file_put_contents(__DIR__ . '/autoload.log', $class . "\n", FILE_APPEND);
    if ($class === 'Warm\\Flaky' && is_file(__DIR__ . '/flaky.fail')) {
        throw new RuntimeException('flaky');
    }
    $file = __DIR__ . '/src/' . str_replace('\\', '/', $class) . '.php';
    if (is_file($file)) {
        require $file;
    }
});
Phera\snapshot();
$name = $_GET['class'] ?? 'Warm\\Thing';
try {
    var_dump(class_exists($name));
    var_dump(class_exists($name, false));
} catch (Throwable $e) {
    echo "caught\n";
}
"#;

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

async fn answer(worker: &WebWorker, site: &Site, script: &Path, target: &str) -> String {
    let response = worker
        .handle_request(script, &site.root, request(target))
        .await;
    String::from_utf8_lossy(&response.body).into_owned()
}

fn thing_class() -> &'static str {
    "<?php\nnamespace Warm;\nclass Thing { const K = 42; }\n"
}

/// The second snapshot declares what requests autoloaded, and a request
/// after it runs no autoloader for the class.
#[tokio::test(flavor = "current_thread")]
async fn second_snapshot_declares_request_classes_and_requests_stop_autoloading() {
    let site = Site::new("settle");
    let script = site.root.join("index.php");
    site.write("index.php", INDEX);
    site.write("src/Warm/Thing.php", thing_class());
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let worker = Runtime::new().web_worker(0).with_background_boots();
            assert!(matches!(
                worker.boot(&script, &site.root).await,
                BootReport::Snapshot { .. }
            ));
            // The first request autoloads Warm\Thing; the fork records it.
            assert_eq!(
                answer(&worker, &site, &script, "/index.php").await,
                "bool(true)\nbool(true)\n"
            );
            assert_eq!(site.log_lines(), 1);
            assert_eq!(
                worker.warmup_classes(&script),
                vec!["Warm\\Thing".to_string()]
            );
            // Requests until the list settles (64 without growth) and the
            // background re-boot has replaced the snapshot: from then on a
            // request finds the class declared and runs no autoloader.
            let mut settled = false;
            for _ in 0..120 {
                let before = site.log_lines();
                let body = answer(&worker, &site, &script, "/index.php").await;
                assert_eq!(body, "bool(true)\nbool(true)\n");
                // Let the LocalSet poll the background boot task.
                for _ in 0..20 {
                    tokio::task::yield_now().await;
                }
                if site.log_lines() == before {
                    settled = true;
                    break;
                }
            }
            assert!(settled, "the worker never re-booted with the warm-up list");
            // One more request on the new snapshot: declared, no autoloader.
            let before = site.log_lines();
            assert_eq!(
                answer(&worker, &site, &script, "/index.php").await,
                "bool(true)\nbool(true)\n"
            );
            assert_eq!(
                site.log_lines(),
                before,
                "the autoloader ran in a request after the re-boot"
            );
        })
        .await;
}

/// With the switch off, no class is collected and every request autoloads.
#[tokio::test(flavor = "current_thread")]
async fn class_warmup_off_keeps_autoloading_every_request() {
    let site = Site::new("off");
    let script = site.root.join("index.php");
    site.write("index.php", INDEX);
    site.write("src/Warm/Thing.php", thing_class());
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let worker = Runtime::new()
                .without_class_warmup(true)
                .web_worker(0)
                .with_background_boots();
            assert!(matches!(
                worker.boot(&script, &site.root).await,
                BootReport::Snapshot { .. }
            ));
            for i in 1..=80 {
                assert_eq!(
                    answer(&worker, &site, &script, "/index.php").await,
                    "bool(true)\nbool(true)\n"
                );
                assert_eq!(site.log_lines(), i, "request {i} ran no autoloader");
            }
            assert!(worker.warmup_classes(&script).is_empty());
        })
        .await;
}

/// A class whose load throws in the boot is dropped from the warm-up and
/// the boot still keeps a snapshot; the request loads it as before.
#[tokio::test(flavor = "current_thread")]
async fn a_class_that_throws_in_the_boot_does_not_break_it() {
    let site = Site::new("flaky");
    let script = site.root.join("index.php");
    site.write("index.php", INDEX);
    site.write(
        "src/Warm/Flaky.php",
        "<?php\nnamespace Warm;\nclass Flaky {}\n",
    );
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let worker = Runtime::new().web_worker(0);
            assert!(matches!(
                worker.boot(&script, &site.root).await,
                BootReport::Snapshot { .. }
            ));
            // The request loads Warm\Flaky; the fork records it.
            assert_eq!(
                answer(&worker, &site, &script, "/index.php?class=Warm\\Flaky").await,
                "bool(true)\nbool(true)\n"
            );
            assert_eq!(
                worker.warmup_classes(&script),
                vec!["Warm\\Flaky".to_string()]
            );
            // Now the autoloader throws for that class, and the worker boots
            // again with the list: the boot must still keep a snapshot.
            site.write("flaky.fail", "1");
            assert!(
                matches!(
                    worker.boot(&script, &site.root).await,
                    BootReport::Snapshot { .. }
                ),
                "the warm-up load of a throwing class failed the boot"
            );
            // The class the boot could not load is dropped from the list.
            assert!(
                worker.warmup_classes(&script).is_empty(),
                "a class whose boot load failed stayed in the list: {:?}",
                worker.warmup_classes(&script)
            );
            // The request loads the class as before: the autoloader throws,
            // the script catches, the class does not exist.
            assert_eq!(
                answer(&worker, &site, &script, "/index.php?class=Warm\\Flaky").await,
                "caught\n"
            );
            assert_eq!(phera_runtime::requests_from_the_top(), 0);
        })
        .await;
}

/// Serves `target` `n` times, letting the LocalSet poll background boots.
async fn serve_n(worker: &WebWorker, site: &Site, script: &Path, target: &str, n: usize) {
    for _ in 0..n {
        let _ = answer(worker, site, script, target).await;
        for _ in 0..5 {
            tokio::task::yield_now().await;
        }
    }
}

/// A worker whose 300 requests all load the same classes boots exactly once
/// more after the list settles, and never again.
#[tokio::test(flavor = "current_thread")]
async fn a_settled_worker_boots_exactly_once_for_the_same_classes() {
    let site = Site::new("once");
    let script = site.root.join("index.php");
    site.write("index.php", INDEX);
    site.write("src/Warm/Thing.php", thing_class());
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let worker = Runtime::new().web_worker(0).with_background_boots();
            assert!(matches!(
                worker.boot(&script, &site.root).await,
                BootReport::Snapshot { .. }
            ));
            serve_n(&worker, &site, &script, "/index.php", 300).await;
            assert_eq!(
                worker.warmup_boots(&script),
                1,
                "300 requests loading the same classes must boot exactly once more"
            );
            // After the warm-up boot a request runs no autoloader.
            let before = site.log_lines();
            assert_eq!(
                answer(&worker, &site, &script, "/index.php").await,
                "bool(true)\nbool(true)\n"
            );
            assert_eq!(
                site.log_lines(),
                before,
                "the autoloader ran after the warm-up boot"
            );
        })
        .await;
}

/// A class first loaded after the first warm-up boot is declared by one more
/// boot; the boot count never passes the maximum.
#[tokio::test(flavor = "current_thread")]
async fn a_new_class_after_the_first_boot_boots_once_more_never_past_the_maximum() {
    let site = Site::new("grow");
    let script = site.root.join("index.php");
    site.write("index.php", INDEX);
    site.write("src/Warm/Thing.php", thing_class());
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let worker = Runtime::new().web_worker(0).with_background_boots();
            assert!(matches!(
                worker.boot(&script, &site.root).await,
                BootReport::Snapshot { .. }
            ));
            // First request loads Warm\Thing; 64 requests without growth
            // settle the list: exactly one warm-up boot.
            serve_n(&worker, &site, &script, "/index.php", 80).await;
            assert_eq!(worker.warmup_boots(&script), 1);
            // Each new class the requests then load is declared by one more
            // boot, until the maximum of 3 warm-up boots is reached.
            for (i, class) in ["Other", "Third", "Fourth"].iter().enumerate() {
                site.write(
                    &format!("src/Warm/{class}.php"),
                    &format!("<?php\nnamespace Warm;\nclass {class} {{}}\n"),
                );
                let target = format!("/index.php?class=Warm\\{class}");
                assert_eq!(
                    answer(&worker, &site, &script, &target).await,
                    "bool(true)\nbool(true)\n"
                );
                serve_n(&worker, &site, &script, "/index.php", 80).await;
                assert_eq!(
                    worker.warmup_boots(&script),
                    (i as u32 + 2).min(3),
                    "after loading {class}, the warm-up boot count"
                );
            }
            assert_eq!(
                worker.warmup_boots(&script),
                3,
                "more than the maximum warm-up boots"
            );
            assert!(
                worker
                    .warmup_classes(&script)
                    .contains(&"Warm\\Fourth".to_string()),
                "the list keeps the class the capped worker never declared: {:?}",
                worker.warmup_classes(&script)
            );
        })
        .await;
}
