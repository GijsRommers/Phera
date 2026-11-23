//! Resolve tags and restore a locked source from the package store's Git mirror.

use phera_packages::{Lock, Options, install, update};
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::Command;

fn git(directory: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command.env_clear().current_dir(directory);
    for name in ["PATH", "HOME", "TMPDIR"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    let result = command
        .args([
            "-c",
            "user.name=Package Test",
            "-c",
            "user.email=package@example.test",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

#[test]
fn installs_git_tags_and_locks_the_commit_for_offline_install() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let repo = root.join("repo");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "--initial-branch=main"]);
    fs::write(repo.join("composer.json"), r#"{"name":"test/git-package"}"#).unwrap();
    fs::write(repo.join("code.php"), "<?php return 12;\n").unwrap();
    git(&repo, &["add", "composer.json", "code.php"]);
    git(&repo, &["commit", "-m", "Fixture"]);
    git(&repo, &["tag", "-a", "1.2.0", "-m", "First release"]);
    let commit = git(&repo, &["rev-parse", "HEAD"]);
    fs::write(
        root.join("composer.json"),
        json!({
            "require": {"test/git-package": "^1"},
            "repositories":[{"type":"vcs","url":"repo"},{"packagist.org":false}],
        })
        .to_string(),
    )
    .unwrap();
    let mut options = Options {
        cache_dir: Some(root.join("cache")),
        no_install: true,
        no_scripts: true,
        ..Options::default()
    };
    update(root, &options).unwrap();
    let lock = Lock::read(root).unwrap();
    assert_eq!(lock.packages[0].version, "1.2.0");
    assert_eq!(lock.packages[0].field("source")["reference"], commit.trim());
    fs::rename(&repo, root.join("repo-offline")).unwrap();
    options.no_install = false;
    options.frozen = true;
    options.offline = true;
    install(root, &options).unwrap();
    let installed = root.join("vendor/test/git-package");
    assert_eq!(
        fs::read(installed.join("code.php")).unwrap(),
        b"<?php return 12;\n"
    );
    assert!(!installed.join(".git").exists());
}

#[test]
fn repository_filters_control_package_and_provider_selection() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let package = |name: &str, version: &str| {
        json!({
            "name":name, "version":version, "type":"metapackage"
        })
    };
    fs::write(
        root.join("composer.json"),
        json!({
            "require": {"test/a":"*", "test/b":"*", "test/c":"*"},
            "repositories": [
                {"type":"package","only":["test/a","test/c"],"exclude":["test/c"],"package":[
                    package("test/a","1.0"),package("test/b","9.0"),package("test/c","9.0")
                ]},
                {"type":"package","canonical":false,"package":[
                    package("test/a","2.0"),package("test/b","2.0"),package("test/c","2.0")
                ]},
                {"type":"package","package":[package("test/b","3.0")]},
                {"packagist.org":false}
            ]
        })
        .to_string(),
    )
    .unwrap();
    update(
        root,
        &Options {
            cache_dir: Some(root.join("cache")),
            no_install: true,
            no_scripts: true,
            ..Options::default()
        },
    )
    .unwrap();
    let lock = Lock::read(root).unwrap();
    let versions: Vec<_> = lock
        .packages
        .iter()
        .map(|package| package.version.as_str())
        .collect();
    assert_eq!(versions, ["1.0", "3.0", "2.0"]);
}
