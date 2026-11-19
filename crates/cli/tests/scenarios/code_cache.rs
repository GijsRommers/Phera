//! A warm server keeps compiled PHP by real path and rechecks include
//! selection when an entry's timestamp interval ends.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use phera_extension_api::testing::oracle::find_php;

use super::super::Served;

const FRONT: &str = r#"<?php
chdir(__DIR__);
set_include_path('preferred:.');
set_error_handler(static function ($number, $message) { echo $message, "\n"; });
try {
    require_once 'item.php';
    require_once './src/../item.php';
} catch (Error $error) {
    echo $error->getMessage();
}
"#;

#[test]
fn development_mode_rechecks_entry_and_includes_on_the_next_request() {
    let served = Served::start_in(
        "development-cache",
        1,
        &[
            ("index.php", "<?php require __DIR__ . '/part.php';"),
            ("part.php", "<?php echo 'original';"),
        ],
        ".",
        &[
            "--dev",
            "-d",
            "opcache.validate_timestamps=0",
            "-d",
            "opcache.revalidate_freq=60",
        ],
    );
    assert_eq!(served.get("/route"), (200, "original".into()));
    std::fs::write(served.root.join("part.php"), "<?php echo 'edited';").unwrap();
    assert_eq!(served.get("/route"), (200, "edited".into()));
    std::fs::write(served.root.join("index.php"), "<?php echo 'new entry';").unwrap();
    assert_eq!(served.get("/route"), (200, "new entry".into()));
}

#[test]
fn edited_added_and_deleted_includes_are_rechecked() {
    let served = Served::start_in(
        "revalidate-includes",
        1,
        &[
            ("index.php", FRONT),
            ("item.php", "<?php echo 'original';"),
            ("src/.keep", ""),
            ("preferred/.keep", ""),
        ],
        ".",
        &["-d", "opcache.revalidate_freq=1"],
    );
    assert_eq!(served.get("/route"), (200, "original".into()));
    assert_eq!(served.get("/route"), (200, "original".into()));

    let item = served.root.join("item.php");
    std::fs::write(&item, "<?php echo 'edited content';").unwrap();
    eventually(&served, "edited content");

    let preferred = served.root.join("preferred/item.php");
    std::fs::write(&preferred, "<?php echo 'preferred ';").unwrap();
    eventually(&served, "preferred edited content");

    std::fs::remove_file(preferred).unwrap();
    std::fs::remove_file(item).unwrap();
    let missing = "require_once(item.php): Failed to open stream: No such file or directory\n\
                   Failed opening required 'item.php' (include_path='preferred:.')";
    if let Some(php) = find_php() {
        let script = served.root.join("index.php");
        assert_eq!(php_output(&php, &script), missing);
    }
    eventually(&served, missing);

    std::fs::write(served.root.join("index.php"), "<?php echo 'new entry';").unwrap();
    eventually(&served, "new entry");
}

fn php_output(php: &Path, script: &Path) -> String {
    let result = Command::new(php).arg("-n").arg(script).output().unwrap();
    assert!(result.status.success());
    String::from_utf8(result.stdout).unwrap()
}

fn eventually(served: &Served, expected: &str) {
    let start = Instant::now();
    loop {
        let (status, body) = served.get("/route");
        assert_eq!(status, 200, "{body}");
        if expected == body {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(4),
            "expected {expected:?}, got {body:?}"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn relative_includes_key_on_caller_cwd_and_include_path() {
    let served = Served::start_in(
        "contextual-includes",
        1,
        &[
            (
                "index.php",
                r#"<?php
chdir(__DIR__ . '/a'); include './item.php';
chdir(__DIR__ . '/b'); include './item.php';
set_include_path(__DIR__ . '/a'); include 'item.php';
set_include_path(__DIR__ . '/b'); include 'item.php';
set_include_path(__DIR__ . '/empty');
include __DIR__ . '/a/caller.php';
include __DIR__ . '/b/caller.php';
"#,
            ),
            ("a/caller.php", "<?php include 'item.php';"),
            ("b/caller.php", "<?php include 'item.php';"),
            ("a/item.php", "<?php echo 'a';"),
            ("b/item.php", "<?php echo 'b';"),
            ("empty/.keep", ""),
        ],
        ".",
        &["-d", "opcache.validate_timestamps=0"],
    );
    let expected = "ababab";
    if let Some(php) = find_php() {
        assert_eq!(php_output(&php, &served.root.join("index.php")), expected);
    }
    assert_eq!(served.get("/route"), (200, expected.into()));
    std::fs::write(served.root.join("a/item.php"), "<?php echo 'changed';").unwrap();
    assert_eq!(served.get("/route"), (200, expected.into()));
}
