//! Generated files for explicitly allowed native Composer plugin hooks.

use phera_packages::{Options, install};
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

fn package(root: &Path, name: &str, kind: &str, files: &[(&str, &str)]) -> Value {
    let directory = format!("packages/{}", name.replace('/', "-"));
    let source = root.join(&directory);
    fs::create_dir_all(&source).unwrap();
    for (path, text) in files {
        let path = source.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    json!({"name": name, "version": "1.0.0", "type": kind,
        "dist": {"type": "path", "url": directory},
        "transport-options": {"symlink": false}})
}

fn lock(root: &Path, packages: &[Value]) {
    fs::write(
        root.join("composer.lock"),
        json!({
            "content-hash": "fixture", "packages": packages, "packages-dev": []
        })
        .to_string(),
    )
    .unwrap();
}

fn options(root: &Path) -> Options {
    Options {
        cache_dir: Some(root.join("cache")),
        ..Options::default()
    }
}

#[test]
fn symfony_runtime_only_generates_for_an_allowed_plugin() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    lock(
        root,
        &[package(
            root,
            "symfony/runtime",
            "composer-plugin",
            &[(
                "Internal/autoload_runtime.template",
                "<?php return [%runtime_class%, %runtime_options%];\n",
            )],
        )],
    );
    fs::write(root.join("composer.json"), "{}").unwrap();
    let options = options(root);
    let error = install(root, &options).unwrap_err();
    assert!(error.to_string().contains("config.allow-plugins"));
    assert!(!root.join("vendor").exists());

    fs::write(
        root.join("composer.json"),
        json!({"config": {"allow-plugins": {
            "symfony/*": false, "symfony/runtime": true
        }}})
        .to_string(),
    )
    .unwrap();
    install(root, &options).unwrap();
    let output = root.join("vendor/autoload_runtime.php");
    assert!(!output.exists());

    fs::write(
        root.join("composer.json"),
        json!({"config": {"allow-plugins": {
        "symfony/runtime": true
    }}, "extra": {"runtime": {"class": "App\\Runtime", "debug": false}}})
        .to_string(),
    )
    .unwrap();
    install(root, &options).unwrap();
    assert_eq!(
        fs::read_to_string(output).unwrap(),
        concat!(
            "<?php return ['App\\\\Runtime', [\n",
            "  'debug' => false,\n",
            "  'project_dir' => dirname(__DIR__, 1),\n",
            "]];\n",
        )
    );
}
