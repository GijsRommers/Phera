//! Offline deterministic installation checks with project-local archives.
//!
//! Run `cargo test -p phera-packages --test locked_install`; no registry is contacted.

use phera_packages::{Options, dump_autoload, install};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use std::fs;
use std::io::{Cursor, Write};
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

fn project() -> TempDir {
    tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap()
}

fn archive(directory: &Path, name: &str, files: &[(&str, &str)]) -> Value {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, contents) in files {
        writer
            .start_file(format!("release/{name}"), SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
    }
    let bytes = writer.finish().unwrap().into_inner();
    let filename = format!("{}.zip", name.replace('/', "-"));
    fs::write(directory.join(&filename), &bytes).unwrap();
    json!({"name": name, "version": "1.0.0", "type": "library",
        "dist": {"type": "zip", "url": format!("file://{}", directory.join(filename).display()),
        "shasum": format!("{:x}", Sha1::digest(&bytes)), "reference": "fixture"}})
}

fn php(directory: &Path, code: &str) -> String {
    let binary = std::env::var_os("PHERA_PHP").unwrap_or_else(|| {
        if Path::new("/opt/homebrew/bin/php").exists() {
            "/opt/homebrew/bin/php".into()
        } else {
            "php".into()
        }
    });
    let result = Command::new(binary)
        .args(["-n", "-r", code])
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "stdout: {} stderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap()
}

#[test]
fn installs_relocatable_composer_autoloader_and_protects_store() {
    let directory = project();
    let root = directory.path();
    let mut dependency = archive(
        root,
        "acme/base",
        &[
            ("src/Thing.php", "<?php namespace Acme; class Thing {}"),
            ("legacy/Legacy/Item.php", "<?php class Legacy_Item {}"),
            (
                "map/types.php",
                "<?php namespace Shapes; interface Shape {} trait T {} enum E {}",
            ),
            ("map/excluded.php", "<?php class Excluded {}"),
            ("files.php", "<?php $GLOBALS['order'][] = 'base';"),
            (
                "bin/tool",
                "<?php echo basename($GLOBALS['_composer_bin_dir']);",
            ),
        ],
    );
    dependency["autoload"] = json!({"psr-4": {"Acme\\": "src/"},
        "psr-0": {"Legacy_": "legacy/"}, "classmap": ["map/"],
        "exclude-from-classmap": ["/map/excluded.php"], "files": ["files.php"]});
    dependency["bin"] = json!(["bin/tool"]);
    let mut consumer = archive(
        root,
        "acme/consumer",
        &[("files.php", "<?php $GLOBALS['order'][] = 'consumer';")],
    );
    consumer["require"] = json!({"acme/base": "*"});
    consumer["autoload"] = json!({"files": ["files.php"]});
    let mut dev = archive(root, "acme/dev", &[("Dev.php", "<?php class Dev {}")]);
    dev["autoload"] = json!({"classmap": ["Dev.php"]});
    let manifest = json!({"name": "test/root", "autoload": {"files": ["root.php"]},
        "autoload-dev": {"classmap": ["RootDev.php"]}});
    fs::write(root.join("composer.json"), manifest.to_string()).unwrap();
    fs::write(root.join("root.php"), "<?php $GLOBALS['order'][] = 'root';").unwrap();
    fs::write(root.join("RootDev.php"), "<?php class RootDev {}").unwrap();
    fs::write(
        root.join("composer.lock"),
        json!({"content-hash": "stale",
        "packages": [consumer, dependency], "packages-dev": [dev]})
        .to_string(),
    )
    .unwrap();
    let mut options = Options {
        cache_dir: Some(root.join("cache")),
        optimize: true,
        no_scripts: true,
        ..Options::default()
    };
    install(root, &options).unwrap();
    let probe = r#"$loader = require 'vendor/autoload.php';
        foreach (['Acme\\Thing', 'Legacy_Item', 'Shapes\\Shape', 'Shapes\\T', 'Shapes\\E',
            'Dev', 'RootDev', 'Excluded', 'Missing'] as $name) {
            echo $name, ':', (int)(false !== $loader->findFile($name)), "\n";
        }
        echo implode(',', $GLOBALS['order']), "\n";
        echo Composer\InstalledVersions::getPrettyVersion('acme/base'), "\n";
        echo (int)Composer\InstalledVersions::isInstalled('acme/dev', false);"#;
    let expected = "Acme\\Thing:1\nLegacy_Item:1\nShapes\\Shape:1\nShapes\\T:1\nShapes\\E:1\n\
        Dev:1\nRootDev:1\nExcluded:0\nMissing:0\nbase,consumer,root\n1.0.0\n0";
    assert_eq!(php(root, probe), expected);
    assert_eq!(php(root, "include 'vendor/bin/tool';"), "bin");
    fs::write(root.join("vendor/acme/base/src/Thing.php"), "corrupted").unwrap();
    fs::remove_dir_all(root.join("vendor")).unwrap();
    options.offline = true;
    let lock_path = root.join("composer.lock");
    let mut lock: Value = serde_json::from_slice(&fs::read(&lock_path).unwrap()).unwrap();
    lock["packages"][0]["description"] = "Metadata changed without changing the archive".into();
    fs::write(&lock_path, lock.to_string()).unwrap();
    install(root, &options).unwrap();
    assert_eq!(php(root, probe), expected);
    let moved = root.join("moved");
    fs::create_dir(&moved).unwrap();
    for name in ["vendor", "root.php", "RootDev.php"] {
        fs::rename(root.join(name), moved.join(name)).unwrap();
    }
    assert_eq!(php(&moved, probe), expected);
    options.no_dev = true;
    install(root, &options).unwrap();
    assert!(!root.join("vendor/acme/dev").exists());
    dump_autoload(root, &options).unwrap();
}

#[test]
fn frozen_and_offline_fail_with_actionable_errors() {
    let directory = project();
    fs::write(directory.path().join("composer.json"), "{}").unwrap();
    fs::write(
        directory.path().join("composer.lock"),
        json!({"content-hash": "stale",
        "packages": [{"name": "acme/missing", "version": "1.0.0"}]})
        .to_string(),
    )
    .unwrap();
    let mut options = Options {
        frozen: true,
        cache_dir: Some(directory.path().join("cache")),
        ..Options::default()
    };
    assert!(
        install(directory.path(), &options)
            .unwrap_err()
            .to_string()
            .contains("out of date")
    );
    options.frozen = false;
    options.offline = true;
    assert!(
        install(directory.path(), &options)
            .unwrap_err()
            .to_string()
            .contains("acme/missing")
    );
}

#[test]
fn no_dev_removes_previous_packages_and_runs_root_events_in_order() {
    let directory = project();
    let root = directory.path();
    let production = archive(root, "acme/base", &[("Base.php", "<?php class Base {}")]);
    let development = archive(root, "acme/dev", &[("Dev.php", "<?php class Dev {}")]);
    fs::write(
        root.join("composer.json"),
        json!({"scripts": {
            "pre-install-cmd": ["@putenv PHERA_SCRIPT_TEST=present", "printf pre > events.txt"],
            "pre-autoload-dump": "@write-autoload",
            "write-autoload": "printf autoload >> events.txt",
            "post-autoload-dump": "test \"$PHERA_SCRIPT_TEST\" = present && printf post >> events.txt",
            "post-install-cmd": "printf install >> events.txt"
        }})
        .to_string(),
    )
    .unwrap();
    fs::write(
        root.join("composer.lock"),
        json!({"content-hash": "stale",
        "packages": [production], "packages-dev": [development]})
        .to_string(),
    )
    .unwrap();
    let mut options = Options {
        cache_dir: Some(root.join("cache")),
        ..Options::default()
    };
    install(root, &options).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("events.txt")).unwrap(),
        "preautoloadpostinstall"
    );
    assert!(root.join("vendor/acme/dev").is_dir());
    options.no_dev = true;
    options.offline = true;
    install(root, &options).unwrap();
    assert!(!root.join("vendor/acme/dev").exists());
    let installed: Value =
        serde_json::from_slice(&fs::read(root.join("vendor/composer/installed.json")).unwrap())
            .unwrap();
    assert_eq!(installed["dev-package-names"], json!([]));
    options.no_scripts = true;
    fs::write(root.join("events.txt"), "unchanged").unwrap();
    install(root, &options).unwrap();
    assert_eq!(
        fs::read_to_string(root.join("events.txt")).unwrap(),
        "unchanged"
    );
}

#[test]
fn rejects_a_dist_checksum_mismatch() {
    let directory = project();
    let root = directory.path();
    let mut package = archive(root, "acme/base", &[("Base.php", "<?php class Base {}")]);
    package["dist"]["shasum"] = "0000000000000000000000000000000000000000".into();
    fs::write(root.join("composer.json"), "{}").unwrap();
    fs::write(
        root.join("composer.lock"),
        json!({"content-hash": "stale",
        "packages": [package]})
        .to_string(),
    )
    .unwrap();
    let options = Options {
        cache_dir: Some(root.join("cache")),
        ..Options::default()
    };
    assert!(format!("{:#}", install(root, &options).unwrap_err()).contains("Checksum mismatch"));
    assert!(!root.join("vendor/acme/base").exists());
}
