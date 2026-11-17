//! Install local Composer repositories, e.g. `{"type":"path","url":"packages/*"}`.
//!
//! These fixtures never contact a registry and plain PHP verifies the resulting autoloader.

use phera_packages::{Options, install};
use serde_json::json;
use std::fs;
use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;

#[test]
fn path_repository_mirrors_current_sources_and_reuses_an_offline_snapshot() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let root = directory.path();
    let source = root.join("packages/local");
    fs::create_dir_all(source.join("src")).unwrap();
    fs::write(
        source.join("composer.json"),
        json!({"name": "acme/local", "version": "1.0.0",
        "autoload": {"psr-4": {"Acme\\": "src/"}}})
        .to_string(),
    )
    .unwrap();
    fs::write(
        source.join("src/Local.php"),
        "<?php namespace Acme; class Local {}",
    )
    .unwrap();
    fs::write(
        root.join("composer.json"),
        json!({"require": {"acme/local": "^1"},
        "repositories": [{"type": "path", "url": "packages/*", "options": {"symlink": false}},
            {"packagist.org": false}]})
        .to_string(),
    )
    .unwrap();
    let mut options = Options {
        cache_dir: Some(root.join("cache")),
        no_scripts: true,
        ..Options::default()
    };
    install(root, &options).unwrap();
    assert_eq!(
        fs::read(root.join("vendor/acme/local/src/Local.php")).unwrap(),
        fs::read(source.join("src/Local.php")).unwrap()
    );
    fs::write(
        source.join("src/Local.php"),
        "<?php namespace Acme; final class Local {}",
    )
    .unwrap();
    install(root, &options).unwrap();
    assert_eq!(
        fs::read(root.join("vendor/acme/local/src/Local.php")).unwrap(),
        fs::read(source.join("src/Local.php")).unwrap()
    );
    fs::remove_dir_all(&source).unwrap();
    fs::remove_dir_all(root.join("vendor")).unwrap();
    options.offline = true;
    install(root, &options).unwrap();
    assert!(
        fs::read_to_string(root.join("vendor/acme/local/src/Local.php"))
            .unwrap()
            .contains("final class")
    );
}

#[test]
fn artifact_repository_resolves_and_installs_a_local_zip() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let root = directory.path();
    fs::create_dir(root.join("artifacts")).unwrap();
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in [
        (
            "release/composer.json",
            r#"{"name":"acme/archive","version":"2.1.0",
            "autoload":{"classmap":["Archive.php"]}}"#,
        ),
        (
            "release/Archive.php",
            "<?php namespace Acme; class Archive {}",
        ),
    ] {
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(contents.as_bytes()).unwrap();
    }
    fs::write(
        root.join("artifacts/package.zip"),
        archive.finish().unwrap().into_inner(),
    )
    .unwrap();
    fs::write(
        root.join("composer.json"),
        json!({"require": {"acme/archive": "^2"},
        "repositories": [{"type": "artifact", "url": "artifacts"},
            {"packagist.org": false}]})
        .to_string(),
    )
    .unwrap();
    let options = Options {
        cache_dir: Some(root.join("cache")),
        no_scripts: true,
        ..Options::default()
    };
    install(root, &options).unwrap();
    let map = fs::read_to_string(root.join("vendor/composer/autoload_classmap.php")).unwrap();
    assert!(map.contains("Acme\\\\Archive"));
    assert!(root.join("vendor/acme/archive/Archive.php").is_file());
}

#[test]
fn default_path_links_are_live_and_removal_preserves_the_source() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let root = directory.path();
    let source = root.join("packages/live");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("composer.json"),
        json!({"name": "acme/live", "version": "1.0.0", "autoload": {
            "classmap": ["."], "exclude-from-classmap": ["/Excluded.php"]}})
        .to_string(),
    )
    .unwrap();
    fs::write(source.join("data.txt"), "first").unwrap();
    fs::write(source.join("Live.php"), "<?php class Live {}").unwrap();
    fs::write(source.join("Excluded.php"), "<?php class Excluded {}").unwrap();
    fs::write(
        root.join("composer.json"),
        json!({"repositories": [
        {"type": "path", "url": "packages/*"}, {"packagist.org": false}]})
        .to_string(),
    )
    .unwrap();
    let options = Options {
        cache_dir: Some(root.join("cache")),
        no_scripts: true,
        ..Options::default()
    };
    phera_packages::add(root, "acme/live:^1", false, &options).unwrap();
    let map = fs::read_to_string(root.join("vendor/composer/autoload_classmap.php")).unwrap();
    assert!(map.contains("'Live'"));
    assert!(!map.contains("'Excluded'"));
    #[cfg(unix)]
    assert!(root.join("vendor/acme/live").is_symlink());
    fs::write(source.join("data.txt"), "second").unwrap();
    #[cfg(unix)]
    assert_eq!(
        fs::read_to_string(root.join("vendor/acme/live/data.txt")).unwrap(),
        "second"
    );
    install(root, &options).unwrap();
    assert!(
        phera_packages::why(root, "acme/live")
            .unwrap()
            .contains("require acme/live ^1")
    );
    assert!(
        phera_packages::outdated(root, &options)
            .unwrap()
            .contains("up to date")
    );
    phera_packages::remove(root, "acme/live", &options).unwrap();
    assert!(!root.join("vendor/acme/live").exists());
    assert!(source.join("data.txt").is_file());
}
