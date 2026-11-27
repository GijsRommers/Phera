//! Files a framework loads by a path it computes at run time, which no
//! `require` in the source names: Laravel's `config/*.php`, Symfony's
//! compiled container in `var/cache/prod`.

use std::path::Path;

/// A detected framework and what it adds to the build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Framework {
    pub(crate) name: &'static str,
    /// Project-relative patterns of the files it includes dynamically.
    pub(crate) patterns: Vec<String>,
    /// Its command-line entry script, next to the web entry.
    pub(crate) console: &'static str,
}

/// Recognises a Laravel or Symfony project by the files that make it one.
/// `vendor` is the vendor directory relative to the project root.
pub(crate) fn detect(root: &Path, vendor: &str) -> Option<Framework> {
    if root.join("artisan").is_file() && root.join("bootstrap/app.php").is_file() {
        return Some(laravel(vendor));
    }
    if root.join(vendor).join("autoload_runtime.php").is_file() || root.join("var/cache").is_dir() {
        return Some(symfony(root));
    }
    None
}

fn laravel(vendor: &str) -> Framework {
    let mut patterns: Vec<String> = [
        "bootstrap/*.php",
        "bootstrap/cache/*.php",
        "config/**/*.php",
        "routes/**/*.php",
        "storage/framework/views/*.php",
        "lang/**/*.php",
    ]
    .map(String::from)
    .to_vec();
    patterns.push(format!(
        "{vendor}/laravel/framework/src/Illuminate/Translation/lang/**/*.php"
    ));
    Framework {
        name: "laravel",
        patterns,
        console: "artisan",
    }
}

fn symfony(root: &Path) -> Framework {
    // A production cache is the one a built image runs with. Without one,
    // every environment that was warmed up is taken.
    let cache = if root.join("var/cache/prod").is_dir() {
        "var/cache/prod/**/*.php"
    } else {
        "var/cache/*/**/*.php"
    };
    Framework {
        name: "symfony",
        patterns: vec![cache.to_owned(), "config/**/*.php".to_owned()],
        console: "bin/console",
    }
}

#[cfg(test)]
mod tests {
    use super::detect;
    use std::path::Path;

    fn touch(root: &Path, name: &str) {
        let file = root.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, "<?php").unwrap();
    }

    #[test]
    fn laravel_needs_artisan_and_its_bootstrap_file() {
        let project = tempfile::tempdir().unwrap();
        touch(project.path(), "artisan");
        assert_eq!(detect(project.path(), "vendor"), None);
        touch(project.path(), "bootstrap/app.php");
        let framework = detect(project.path(), "vendor").unwrap();
        assert_eq!(framework.name, "laravel");
        assert_eq!(framework.console, "artisan");
        assert!(framework.patterns.contains(&"config/**/*.php".to_owned()));
        assert!(framework.patterns.contains(
            &"vendor/laravel/framework/src/Illuminate/Translation/lang/**/*.php".to_owned()
        ));
    }

    #[test]
    fn symfony_prefers_the_production_cache() {
        let project = tempfile::tempdir().unwrap();
        touch(project.path(), "vendor/autoload_runtime.php");
        let framework = detect(project.path(), "vendor").unwrap();
        assert_eq!(framework.name, "symfony");
        assert_eq!(
            framework.patterns,
            ["var/cache/*/**/*.php", "config/**/*.php"]
        );
        touch(project.path(), "var/cache/prod/url_matching_routes.php");
        let framework = detect(project.path(), "vendor").unwrap();
        assert_eq!(framework.patterns[0], "var/cache/prod/**/*.php");
    }

    #[test]
    fn a_plain_project_has_no_framework() {
        let project = tempfile::tempdir().unwrap();
        touch(project.path(), "index.php");
        assert_eq!(detect(project.path(), "vendor"), None);
    }
}
