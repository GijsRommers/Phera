//! Resolve synthetic Composer repositories served on loopback inside each test.
//!
//! No Packagist dependency: the local registry tests backtracking, virtual packages and caches.

use phera_packages::{Options, update};
use serde_json::{Value, json};
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

struct Registry {
    url: String,
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Registry {
    fn start(packages: Value) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let (stop, receiver) = mpsc::channel();
        let bytes = json!({"packages": packages}).to_string();
        let thread = std::thread::spawn(move || {
            loop {
                if receiver.recv_timeout(Duration::from_millis(5)).is_ok() {
                    break;
                }
                if let Ok((mut stream, _)) = listener.accept() {
                    stream
                        .set_read_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let mut request = [0; 4096];
                    let _ = stream.read(&mut request);
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                    Content-Length: {}\r\nETag: \"fixture\"\r\nConnection: close\r\n\r\n{}",
                        bytes.len(),
                        bytes
                    )
                    .unwrap();
                }
            }
        });
        Self {
            url,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Registry {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

fn package(name: &str, version: &str, extra: Value) -> Value {
    let mut package = json!({"name": name, "version": version, "type": "metapackage"});
    package
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    package
}

fn resolve(require: Value, packages: Value, extra: Value, lowest: bool) -> Value {
    let registry = Registry::start(packages);
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let mut manifest = json!({"name": "test/root", "require": require,
        "repositories": [{"type": "composer", "url": registry.url}, {"packagist.org": false}]});
    manifest
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    fs::write(directory.path().join("composer.json"), manifest.to_string()).unwrap();
    let mut options = Options {
        cache_dir: Some(directory.path().join("cache")),
        prefer_lowest: lowest,
        no_scripts: true,
        ..Options::default()
    };
    update(directory.path(), &options).unwrap();
    let initial: Value =
        serde_json::from_slice(&fs::read(directory.path().join("composer.lock")).unwrap()).unwrap();
    drop(registry);
    options.offline = true;
    update(directory.path(), &options).unwrap();
    let offline: Value =
        serde_json::from_slice(&fs::read(directory.path().join("composer.lock")).unwrap()).unwrap();
    assert_eq!(initial, offline);
    initial["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|package| {
            (
                package["name"].as_str().unwrap().to_owned(),
                package["version"].clone(),
            )
        })
        .collect::<serde_json::Map<String, Value>>()
        .into()
}

#[test]
fn backtracks_conflicts_and_cycles() {
    let packages = json!({"acme/a": {
        "1.0.0": package("acme/a", "1.0.0", json!({"require": {"acme/b": "^1"}})),
        "2.0.0": package("acme/a", "2.0.0", json!({"require": {"acme/b": "^2"}}))},
        "acme/b": {"1.0.0": package("acme/b", "1.0.0", json!({"require": {"acme/a": "*"}})),
            "2.0.0": package("acme/b", "2.0.0", json!({}))}});
    assert_eq!(
        resolve(
            json!({"acme/a": "*", "acme/b": "^1"}),
            packages,
            json!({}),
            false
        ),
        json!({"acme/a": "1.0.0", "acme/b": "1.0.0"})
    );
    let packages = json!({"acme/a": {"1": package("acme/a", "1.0.0",
        json!({"conflict": {"acme/b": "^2"}}))}, "acme/b": {
            "1": package("acme/b", "1.0.0", json!({})),
            "2": package("acme/b", "2.0.0", json!({}))}});
    assert_eq!(
        resolve(
            json!({"acme/a": "*", "acme/b": "*"}),
            packages,
            json!({}),
            false
        ),
        json!({"acme/a": "1.0.0", "acme/b": "1.0.0"})
    );
}

#[test]
fn replaces_and_provides_virtual_packages() {
    for key in ["replace", "provide"] {
        let packages = json!({"acme/a": {"1": package("acme/a", "1.0.0",
            json!({key: {"acme/virtual": "self.version"}}))}});
        assert_eq!(
            resolve(
                json!({"acme/a": "*", "acme/virtual": "^1"}),
                packages,
                json!({}),
                false
            ),
            json!({"acme/a": "1.0.0"})
        );
    }
}

#[test]
fn stability_lowest_platform_and_branch_aliases() {
    let packages = json!({"acme/a": {
        "1": package("acme/a", "1.0.0", json!({"require": {"php": "^8.5"}})),
        "2": package("acme/a", "2.0.0-beta", json!({})),
        "3": package("acme/a", "dev-main", json!({"extra": {
            "branch-alias": {"dev-main": "3.x-dev"}}}))}});
    let config = json!({"minimum-stability": "dev", "prefer-stable": true,
        "config": {"platform": {"php": "8.5.12"}}});
    assert_eq!(
        resolve(json!({"acme/a": "*"}), packages.clone(), config, false),
        json!({"acme/a": "1.0.0"})
    );
    assert_eq!(
        resolve(
            json!({"acme/a": "^3@dev"}),
            packages.clone(),
            json!({}),
            false
        ),
        json!({"acme/a": "dev-main"})
    );
    assert_eq!(
        resolve(
            json!({"acme/a": ">=1@dev"}),
            packages,
            json!({"config": {"platform": {"php": "8.5.12"}}}),
            true
        ),
        json!({"acme/a": "1.0.0"})
    );
}

#[test]
fn inline_aliases_and_the_root_package_satisfy_transitive_requirements() {
    let packages = json!({"acme/a": {"dev-main": package("acme/a", "dev-main", json!({}))},
        "acme/b": {"1": package("acme/b", "1.0.0", json!({"require": {
            "acme/a": "^2", "test/root": "^1"}}))}});
    assert_eq!(
        resolve(
            json!({"acme/a": "dev-main as 2.0.0", "acme/b": "*"}),
            packages,
            json!({}),
            false
        ),
        json!({"acme/a": "dev-main", "acme/b": "1.0.0"})
    );
}

#[test]
fn reports_unsatisfied_platform_without_writing_a_lock() {
    let registry = Registry::start(json!({}));
    let directory = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    fs::write(
        directory.path().join("composer.json"),
        json!({"name": "test/root",
        "require": {"php": "^9"}, "config": {"platform": {"php": "8.5.12"}},
        "repositories": [{"type": "composer", "url": registry.url},
            {"packagist.org": false}]})
        .to_string(),
    )
    .unwrap();
    let options = Options {
        cache_dir: Some(directory.path().join("cache")),
        no_scripts: true,
        ..Options::default()
    };
    let error = update(directory.path(), &options).unwrap_err().to_string();
    assert!(error.contains("php ^9"), "{error}");
    assert!(!directory.path().join("composer.lock").exists());
}
