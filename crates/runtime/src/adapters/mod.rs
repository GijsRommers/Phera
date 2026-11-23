//! Adapters: Laravel and Symfony applications snapshot with no change.
//!
//! ```text
//! my-app/artisan, my-app/bootstrap/app.php   -> Laravel: phera serve runs
//!                                               laravel.php for public/index.php
//! my-app/vendor/autoload_runtime.php          -> symfony/runtime: phera serve
//!                                               sets APP_RUNTIME=Phera\Symfony\Runtime
//! ```
//!
//! Each adapter boots the framework (the kernel and its providers or
//! bundles), warms it up, and calls `Phera\snapshot()` before it looks at
//! the request. Both are PHP shipped in the binary: Laravel's as an entry
//! script that is compiled under the name of the application's
//! `public/index.php` (so `__DIR__` and stack traces are the
//! application's), Symfony's as a class the VM compiles when
//! `symfony/runtime` asks for it (see `phera_vm`'s bundled classes).
//!
//! The warm-up builds, before the snapshot, what nearly every request would
//! build first (`LaravelWarmUp.php`, and `warmUp()` in `SymfonyRuntime.php`,
//! say what and why it is safe without a request):
//!
//! ```text
//! phera serve public                       -> services resolved, models booted
//! phera serve --no-service-warmup public   -> snapshot right after the boot
//! ```

use std::path::Path;

use phera_extension_api::{BundledSource, Registry};

/// The entry script for a Laravel application.
pub const LARAVEL_ENTRY: &str = include_str!("laravel.php");

/// The environment variable both adapters read before they warm up: the
/// value `0` (which `phera serve --no-service-warmup` sets) makes them take
/// the snapshot right after the framework has booted.
pub const SERVICE_WARMUP_VARIABLE: &[u8] = b"PHERA_SERVICE_WARMUP";

/// The class `APP_RUNTIME` names for an application using symfony/runtime.
pub const SYMFONY_RUNTIME_CLASS: &str = "Phera\\Symfony\\Runtime";

const SYMFONY_RUNTIME: BundledSource = BundledSource {
    file: "phera://symfony/Runtime.php",
    source: include_str!("SymfonyRuntime.php"),
};

const LARAVEL_WARM_UP: BundledSource = BundledSource {
    file: "phera://laravel/WarmUp.php",
    source: include_str!("LaravelWarmUp.php"),
};

/// Ships the Symfony runtime's classes and Laravel's warm-up.
pub(crate) fn register(registry: &mut Registry) {
    registry.bundled_class("Phera\\Laravel\\WarmUp", LARAVEL_WARM_UP);
    registry.bundled_class(SYMFONY_RUNTIME_CLASS, SYMFONY_RUNTIME);
    registry.bundled_class("Phera\\Symfony\\SnapshotRunner", SYMFONY_RUNTIME);
}

/// Whether the project around `document_root` is a Laravel application:
/// `artisan` and `bootstrap/app.php` in the directory above it.
pub fn is_laravel(document_root: &Path) -> bool {
    document_root.parent().is_some_and(|project| {
        project.join("artisan").is_file() && project.join("bootstrap").join("app.php").is_file()
    })
}

/// Whether the project around `document_root` boots through symfony/runtime.
pub fn uses_symfony_runtime(document_root: &Path) -> bool {
    document_root.parent().is_some_and(|project| {
        project
            .join("vendor")
            .join("autoload_runtime.php")
            .is_file()
    })
}

#[cfg(test)]
mod tests {
    use super::{LARAVEL_ENTRY, LARAVEL_WARM_UP, SYMFONY_RUNTIME};

    /// The shipped PHP is compiled when an application first needs it, in a
    /// worker's boot. This repository's own tests hold no framework, so
    /// without this test a syntax error would first show there.
    #[test]
    fn the_shipped_php_compiles() {
        for (file, source) in [
            ("laravel.php", LARAVEL_ENTRY),
            (LARAVEL_WARM_UP.file, LARAVEL_WARM_UP.source),
            (SYMFONY_RUNTIME.file, SYMFONY_RUNTIME.source),
        ] {
            if let Err(error) = phera_compiler::compile(source, file) {
                panic!("{file} does not compile: {error:?}");
            }
        }
    }

    /// Laravel's own `public/index.php` calls `handleRequest()` on line 20,
    /// and the entry is compiled under that file's name: a stack trace of
    /// the application must name the line a developer finds in their file.
    #[test]
    fn laravel_handles_the_request_on_the_line_of_its_own_front_controller() {
        let line = LARAVEL_ENTRY
            .lines()
            .position(|line| line.contains("$app->handleRequest("));
        assert_eq!(line, Some(19), "counted from zero");
    }
}
