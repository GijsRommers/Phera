//! Lock installs validate PHP, development extensions and virtual platform providers.
//!
//! Run `cargo test -p phera-packages --test locked_platform` without a registry.

use phera_packages::{Options, install};
use serde_json::json;
use std::fs;

#[test]
fn rejects_incompatible_platform_before_creating_vendor_and_honors_no_dev() {
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let root = directory.path();
    let mut manifest = json!({"name": "acme/root", "require": {"php": "^8.5"},
        "require-dev": {"ext-example": "^1"}});
    let mut lock = json!({"content-hash": "fixture", "packages": [], "packages-dev": []});
    fs::write(root.join("composer.json"), manifest.to_string()).unwrap();
    fs::write(root.join("composer.lock"), lock.to_string()).unwrap();
    let mut options = Options {
        platform: [("php".into(), "8.4.0".into())].into_iter().collect(),
        cache_dir: Some(root.join("cache")),
        no_scripts: true,
        ..Options::default()
    };
    let error = install(root, &options).unwrap_err().to_string();
    assert!(error.contains("requires php ^8.5"));
    assert!(!root.join("vendor").exists());

    manifest["config"] = json!({"platform": {"php": "8.5.0"}});
    fs::write(root.join("composer.json"), manifest.to_string()).unwrap();
    let error = install(root, &options).unwrap_err().to_string();
    assert!(error.contains("requires ext-example ^1"));
    options.no_dev = true;
    install(root, &options).unwrap();

    options.no_dev = false;
    lock["packages"] = json!([{"name": "acme/polyfill", "version": "1.0.0",
        "type": "metapackage", "provide": {"ext-example": "1.0"}}]);
    fs::write(root.join("composer.lock"), lock.to_string()).unwrap();
    install(root, &options).unwrap();

    manifest["conflict"] = json!({"php": "^8.5"});
    fs::write(root.join("composer.json"), manifest.to_string()).unwrap();
    let error = install(root, &options).unwrap_err().to_string();
    assert!(error.contains("conflicts with php ^8.5"));
    options.ignore_platform = true;
    install(root, &options).unwrap();
}
