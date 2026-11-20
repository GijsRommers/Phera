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

#[test]
fn framework_locations_are_shared_by_install_metadata_and_autoload() {
    let project = tempfile::tempdir().unwrap();
    let root = project.path();
    let mut drupal = package(
        root,
        "drupal/core",
        "drupal-core",
        &[("lib/Kernel.php", "<?php namespace Drupal; class Kernel {}")],
    );
    drupal["autoload"] = json!({"psr-4": {"Drupal\\": "lib/"}});
    lock(
        root,
        &[
            package(root, "composer/installers", "composer-plugin", &[]),
            package(
                root,
                "roots/wordpress-core-installer",
                "composer-plugin",
                &[],
            ),
            package(
                root,
                "roots/wordpress-no-content",
                "wordpress-core",
                &[("wp-blog-header.php", "<?php return true;")],
            ),
            drupal,
        ],
    );
    let manifest = json!({"config": {"allow-plugins": {
        "composer/installers": true, "roots/wordpress-core-installer": true
    }}, "extra": {
        "installer-paths": {"web/core": ["type:drupal-core"]},
        "wordpress-install-dir": {"roots/wordpress-no-content": "web/wp"}
    }});
    fs::write(root.join("composer.json"), manifest.to_string()).unwrap();
    fs::create_dir_all(root.join("web/wp")).unwrap();
    let sentinel = root.join("web/wp/local-file.php");
    fs::write(&sentinel, "local").unwrap();
    let options = options(root);
    let error = install(root, &options).unwrap_err();
    assert!(error.to_string().contains("existing custom path"));
    assert_eq!(fs::read_to_string(&sentinel).unwrap(), "local");
    assert!(!root.join("vendor").exists());
    fs::remove_dir_all(root.join("web/wp")).unwrap();

    install(root, &options).unwrap();
    install(root, &options).unwrap();
    assert!(root.join("web/wp/wp-blog-header.php").is_file());
    assert!(root.join("web/core/lib/Kernel.php").is_file());
    assert!(!root.join("vendor/drupal/core").exists());
    let records = fs::read_to_string(root.join("vendor/composer/installed.json")).unwrap();
    assert!(records.contains("../../web/core"));
    assert!(records.contains("../../web/wp"));
    let psr4 = fs::read_to_string(root.join("vendor/composer/autoload_psr4.php")).unwrap();
    assert!(psr4.contains("/web/core/lib"));
}
