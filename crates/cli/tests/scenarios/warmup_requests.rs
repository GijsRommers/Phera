//! The warm-up requests of `phera serve` are declared with `--warmup` or in
//! the project's `phera.toml`, and reach the framework adapters as one
//! environment variable:
//!
//! ```toml
//! [server]
//! warmup = ["/api/posts?page=1", "/"]
//! ```
//!
//! ```php
//! // in Phera's Laravel warm-up and Symfony runner
//! $paths = preg_split('/\s+/', $_SERVER['PHERA_WARMUP'] ?? '', -1, PREG_SPLIT_NO_EMPTY);
//! ```

use std::process::Command;

use super::super::Served;

const INDEX: &str = "<?php echo $_SERVER['PHERA_WARMUP'] ?? 'not set';";
const FROM_FILE: &str = "[server]\nwarmup = [\"/api/posts?page=1\", \"/\"]\n";

#[test]
fn nothing_is_declared_by_default() {
    let served = Served::start("warmup-none", &[("public/index.php", INDEX)]);
    assert_eq!(served.get("/index.php"), (200, "not set".to_string()));
}

#[test]
fn the_projects_file_declares_the_paths() {
    let served = Served::start_in(
        "warmup-file",
        1,
        &[("phera.toml", FROM_FILE), ("public/index.php", INDEX)],
        "public",
        &[],
    );
    assert_eq!(served.get("/"), (200, "/api/posts?page=1 /".to_string()));
}

#[test]
fn the_option_wins_over_the_file_and_may_be_repeated() {
    let served = Served::start_in(
        "warmup-option",
        1,
        &[("phera.toml", FROM_FILE), ("public/index.php", INDEX)],
        "public",
        &["--warmup", "/health", "--warmup", "/api/hello"],
    );
    assert_eq!(served.get("/"), (200, "/health /api/hello".to_string()));
}

#[test]
fn a_path_must_start_with_a_slash_and_hold_no_space() {
    let phera = env!("CARGO_BIN_EXE_phera");
    for path in ["api/posts", "/a b"] {
        let output = Command::new(phera)
            .args(["serve", "--warmup", path, "."])
            .output()
            .expect("run phera");
        assert!(!output.status.success(), "{path} was accepted");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("--warmup needs a path"),
            "{path}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let root = Served::root_of("warmup-bad-file");
    std::fs::create_dir_all(root.join("public")).expect("create the site");
    std::fs::write(
        root.join("phera.toml"),
        "[server]\nwarmup = \"/api/posts\"\n",
    )
    .expect("write");
    std::fs::write(root.join("public/index.php"), INDEX).expect("write");
    let output = Command::new(phera)
        .arg("serve")
        .arg(root.join("public"))
        .output()
        .expect("run phera");
    let _ = std::fs::remove_dir_all(&root);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("server.warmup must be a list of paths"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
