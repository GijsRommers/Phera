//! Check partial updates, hash refreshes and manifest edits with inline packages.

use phera_packages::{Options, add_many, remove_many, update, update_selected};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

fn options(directory: &Path) -> Options {
    Options {
        cache_dir: Some(directory.join("cache")),
        no_install: true,
        ..Options::default()
    }
}

fn package(name: &str, version: &str, requires: Value) -> Value {
    json!({"name": name, "version": version, "type": "metapackage", "require": requires})
}

fn manifest(directory: &Path, packages: Vec<Value>, root: Value) {
    let path = directory.join("composer.json");
    let value = json!({
        "name": "example/project",
        "require": root,
        "repositories": [{"type": "package", "package": packages}, {"packagist.org": false}],
    });
    fs::write(path, value.to_string()).unwrap();
}

fn load(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

#[test]
fn selected_update_unlocks_only_the_requested_package_and_optional_dependencies() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let mut options = options(root);
    let mut packages = vec![
        package("test/a", "1.0.0", json!({})),
        package("test/b", "1.0.0", json!({})),
        package("test/c", "1.0.0", json!({})),
    ];
    let requirements = json!({"test/a": "*", "test/b": "*", "test/c": "*"});
    manifest(root, packages.clone(), requirements.clone());
    update(root, &options).unwrap();
    let first = load(&root.join("composer.lock"));
    packages.extend([
        package("test/a", "2.0.0", json!({"test/b": "^2"})),
        package("test/b", "2.0.0", json!({})),
        package("test/c", "2.0.0", json!({})),
    ]);
    manifest(root, packages, requirements);
    let subject = ["test/a".to_owned()];
    update_selected(root, &subject, &options).unwrap();
    let pinned = load(&root.join("composer.lock"));
    assert_eq!(first["packages"], pinned["packages"]);
    options.with_all_dependencies = true;
    update_selected(root, &subject, &options).unwrap();
    let unlocked = load(&root.join("composer.lock"));
    assert_eq!(unlocked["packages"][0]["version"], "2.0.0");
    assert_eq!(unlocked["packages"][1]["version"], "2.0.0");
    assert_eq!(unlocked["packages"][2]["version"], "1.0.0");
}

#[test]
fn dry_run_and_failed_edits_preserve_files_and_hash_refresh_preserves_packages() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let mut options = options(root);
    let packages = vec![
        package("test/a", "1.0.0", json!({})),
        package("test/b", "2.0.0", json!({})),
    ];
    manifest(root, packages, json!({}));
    let manifest_path = root.join("composer.json");
    let lock_path = root.join("composer.lock");
    let specs = ["test/a".to_owned(), "test/b:^2".to_owned()];
    let original_manifest = fs::read(&manifest_path).unwrap();
    options.dry_run = true;
    add_many(root, &specs, true, &options).unwrap();
    assert_eq!(fs::read(&manifest_path).unwrap(), original_manifest);
    assert!(!lock_path.exists());
    options.dry_run = false;
    add_many(root, &specs, true, &options).unwrap();
    let mut data = load(&manifest_path);
    assert_eq!(data["require-dev"], json!({"test/a":"^1.0", "test/b":"^2"}));
    let lock_before = fs::read(&lock_path).unwrap();
    let impossible = ["test/a:^99".to_owned()];
    let edit_before = fs::read(&manifest_path).unwrap();
    assert!(add_many(root, &impossible, true, &options).is_err());
    assert_eq!(fs::read(&manifest_path).unwrap(), edit_before);
    assert_eq!(fs::read(&lock_path).unwrap(), lock_before);
    data["require"]["test/a"] = "^1".into();
    fs::write(&manifest_path, data.to_string()).unwrap();
    options.refresh_lock = true;
    update(root, &options).unwrap();
    let mut before: Value = serde_json::from_slice(&lock_before).unwrap();
    let mut after = load(&lock_path);
    assert_ne!(before["content-hash"], after["content-hash"]);
    before.as_object_mut().unwrap().shift_remove("content-hash");
    after.as_object_mut().unwrap().shift_remove("content-hash");
    assert_eq!(before, after);
    options.refresh_lock = false;
    remove_many(root, &["test/a".into(), "test/b".into()], &options).unwrap();
    assert_eq!(load(&lock_path)["packages-dev"], json!([]));
}
