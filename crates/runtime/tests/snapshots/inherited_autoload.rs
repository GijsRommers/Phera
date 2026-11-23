//! A class declared before what it extends, in every request of a worker.
//!
//! ```php
//! spl_autoload_register(fn ($name) => require "$name.php");
//! require 'Manager.php';   // class Manager { use RebindsCallbacks; }
//!                          // the trait is autoloaded while Manager is declared
//! ```
//!
//! The worker keeps the class each request linked; a later request declares
//! the class before its trait, parent or interface exists in that request.
//! The autoloader is called for that name exactly as in the first request,
//! once and in the same order, and the class behaves the same.

use crate::{Site, worker_answers};
use phera_runtime::Runtime;

/// The autoloader logs each name it is called for; the page prints the log
/// and uses the classes.
fn write_site(site: &Site) {
    let file = |name: &str, body: &str| {
        std::fs::write(site.root.join(name), body).expect("write a class file");
    };
    file(
        "index.php",
        "<?php $log = []; spl_autoload_register(function ($name) use (&$log) { $log[] = $name; \
         $file = __DIR__ . '/' . $name . '.php'; if (is_file($file)) { require $file; } }); \
         if (isset($_GET['missing'])) { try { require __DIR__ . '/Orphan.php'; } catch (Error $e) \
         { echo get_class($e), ': ', $e->getMessage(), ' | '; } } \
         require __DIR__ . '/Manager.php'; require __DIR__ . '/Child.php'; \
         echo implode(',', $log), ' | ', (new Manager)->rebind(), ' ', (new Child)->name(), ' ', \
         (new Child) instanceof Named ? 'named' : 'unnamed';",
    );
    file(
        "Manager.php",
        "<?php class Manager { use RebindsCallbacks; }",
    );
    file(
        "RebindsCallbacks.php",
        "<?php trait RebindsCallbacks { function rebind() { return 'rebound'; } }",
    );
    file(
        "Child.php",
        "<?php class Child extends Base implements Named { function name() { return 'child of ' . parent::name(); } }",
    );
    file(
        "Base.php",
        "<?php class Base { function name() { return 'base'; } }",
    );
    file("Named.php", "<?php interface Named {}");
    file("Orphan.php", "<?php class Orphan extends NoSuchParent {}");
}

/// Every request autoloads the trait, the parent and the interface in the
/// same order and gets the same classes.
#[tokio::test(flavor = "current_thread")]
async fn a_class_declared_before_its_trait_parent_and_interface_autoloads_them_each_request() {
    let site = Site::new("inherited-autoload");
    write_site(&site);
    let answers = worker_answers(Runtime::new(), &site, &["index.php"; 4]).await;
    assert_eq!(
        answers,
        ["RebindsCallbacks,Base,Named | rebound child of base named"; 4]
    );
}

/// A parent the autoloader cannot provide is the same error in every
/// request, with the autoloader called once for it each time, whether or
/// not an earlier request of the worker declared the class.
#[tokio::test(flavor = "current_thread")]
async fn a_missing_parent_is_the_same_error_in_every_request() {
    let site = Site::new("inherited-autoload-missing");
    write_site(&site);
    let answers = worker_answers(
        Runtime::new(),
        &site,
        &["index.php?missing=1", "index.php", "index.php?missing=1"],
    )
    .await;
    let missing = "Error: Class \"NoSuchParent\" not found | \
                   NoSuchParent,RebindsCallbacks,Base,Named | rebound child of base named";
    assert_eq!(
        answers,
        [
            missing,
            "RebindsCallbacks,Base,Named | rebound child of base named",
            missing
        ]
    );
}
