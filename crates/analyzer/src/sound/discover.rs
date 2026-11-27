//! Which files belong to a build, before any of them is compiled: the entry
//! scripts, everything Composer can autoload, what a framework includes by
//! a computed path, and what `[build].include` names.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use super::composer::{self, ComposerData};
use super::framework::{self, Framework};
use super::glob::{self, GlobSet};
use super::{BuildConfig, DiscoveredFile, DiscoveredWorld, relative_path};

/// Why a file is in the build, which decides the filters it must pass.
/// Entry scripts pass none: they are added as they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    /// Named by `[build].include`, or loaded on every request: only
    /// `[build].exclude` removes it.
    Requested,
    /// Found by scanning Composer's or a framework's directories: also
    /// removed when it is development-only code and the build leaves that out.
    Scanned,
}

#[derive(Debug, Clone, Copy, Default)]
struct Flags {
    always_loaded: bool,
    is_entry: bool,
}

/// The files found so far, by canonical path, and the filters they passed.
struct Collector<'a> {
    root: &'a Path,
    exclude: GlobSet,
    /// Empty when the build includes development code.
    dev_paths: Vec<PathBuf>,
    files: BTreeMap<PathBuf, Flags>,
    /// Paths a table named that are not on disk, spelled without `..`, so
    /// that a path named by two tables counts once.
    missing: BTreeSet<PathBuf>,
}

pub(crate) fn discover(config: &BuildConfig) -> Result<DiscoveredWorld, String> {
    let root = config
        .project_root
        .canonicalize()
        .map_err(|error| format!("{}: {error}", config.project_root.display()))?;
    if !root.is_dir() {
        return Err(format!("{}: not a directory", root.display()));
    }
    let composer = composer::read(&root)?;
    let vendor = composer.as_ref().map_or_else(
        || "vendor".to_owned(),
        |data| relative_path(&root, &data.vendor_dir),
    );
    let framework = framework::detect(&root, &vendor);
    let dev_paths = match &composer {
        Some(data) if !config.include_dev => data.dev_paths.clone(),
        _ => Vec::new(),
    };
    let mut collector = Collector {
        root: &root,
        exclude: GlobSet::new(&config.exclude_globs),
        dev_paths,
        files: BTreeMap::new(),
        missing: BTreeSet::new(),
    };

    for entry in entry_scripts(config, &root, framework.as_ref())? {
        collector.files.entry(entry).or_default().is_entry = true;
    }
    let mut autoload_files = Vec::new();
    let mut classmap = BTreeMap::new();
    if let Some(composer) = &composer {
        collector.add_composer(composer, &mut autoload_files, &mut classmap);
    }
    if let Some(framework) = &framework {
        for pattern in &framework.patterns {
            collector.add_pattern(pattern, Origin::Scanned);
        }
    }
    for pattern in &config.include_globs {
        collector.add_pattern(pattern, Origin::Requested);
    }

    let skipped_missing = collector.missing.len();
    let mut files: Vec<DiscoveredFile> = collector
        .files
        .into_iter()
        .map(|(path, flags)| DiscoveredFile {
            relative_path: relative_path(&root, &path),
            path,
            always_loaded: flags.always_loaded,
            is_entry: flags.is_entry,
        })
        .collect();
    sort_files(&mut files);
    Ok(DiscoveredWorld {
        project_root: root,
        files,
        autoload_files,
        classmap,
        framework: framework.map(|framework| framework.name),
        skipped_missing,
    })
}

/// The order of a build's files: by relative path, the same on every run.
pub(crate) fn sort_files(files: &mut [DiscoveredFile]) {
    files.sort_by(|a, b| {
        a.relative_path
            .cmp(&b.relative_path)
            .then_with(|| a.path.cmp(&b.path))
    });
}

/// The scripts a request or a command starts in. Without `entries` in the
/// configuration: the web front controller, and the framework's console.
fn entry_scripts(
    config: &BuildConfig,
    root: &Path,
    framework: Option<&Framework>,
) -> Result<Vec<PathBuf>, String> {
    if !config.entries.is_empty() {
        return config
            .entries
            .iter()
            .map(|entry| {
                let path = root.join(entry);
                match path.canonicalize() {
                    Ok(canonical) if canonical.is_file() => Ok(canonical),
                    _ => Err(format!("Entry script does not exist: {}", path.display())),
                }
            })
            .collect();
    }
    let web = ["public/index.php", "index.php"]
        .into_iter()
        .map(|name| root.join(name))
        .find(|path| path.is_file());
    let console = framework
        .map(|framework| root.join(framework.console))
        .filter(|path| path.is_file());
    Ok(web
        .into_iter()
        .chain(console)
        .filter_map(|path| path.canonicalize().ok())
        .collect())
}

impl Collector<'_> {
    /// Adds a file that a table or a pattern names. Its canonical path is
    /// returned when the file exists and passes the filters of `origin`.
    fn add_referenced(&mut self, path: &Path, origin: Origin) -> Option<PathBuf> {
        match path.canonicalize() {
            Ok(canonical) => self.add_canonical(canonical, origin),
            Err(_) => {
                self.missing.insert(without_dot_components(path));
                None
            }
        }
    }

    fn add_canonical(&mut self, canonical: PathBuf, origin: Origin) -> Option<PathBuf> {
        if !canonical.is_file() || !self.admits(&canonical, origin) {
            return None;
        }
        self.files.entry(canonical.clone()).or_default();
        Some(canonical)
    }

    fn admits(&self, canonical: &Path, origin: Origin) -> bool {
        let excluded = || self.exclude.matches(&relative_path(self.root, canonical));
        let development = || self.dev_paths.iter().any(|dev| canonical.starts_with(dev));
        match origin {
            Origin::Requested => !excluded(),
            Origin::Scanned => !excluded() && !development(),
        }
    }

    /// The canonical path of every file below `directory` that passes the
    /// filters of `origin`, sorted. A directory that symbolic links reach
    /// twice is entered once.
    fn files_below(&self, directory: &Path, origin: Origin) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut visited = BTreeSet::new();
        let mut pending = vec![directory.to_path_buf()];
        while let Some(directory) = pending.pop() {
            let Ok(directory) = directory.canonicalize() else {
                continue;
            };
            if !self.admits(&directory, origin) || !visited.insert(directory.clone()) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                // `metadata` follows a symbolic link to what it names.
                let Ok(metadata) = path.metadata() else {
                    continue;
                };
                if metadata.is_dir() {
                    pending.push(path);
                } else if metadata.is_file()
                    && let Ok(canonical) = path.canonicalize()
                    && self.admits(&canonical, origin)
                {
                    files.push(canonical);
                }
            }
        }
        files.sort();
        files.dedup();
        files
    }

    /// Adds what a pattern names: one file, every PHP file of a directory,
    /// or the files a wildcard matches.
    fn add_pattern(&mut self, pattern: &str, origin: Origin) {
        let pattern = glob::normalize(pattern);
        if pattern.is_empty() {
            return;
        }
        let base = self.root.join(glob::literal_prefix(&pattern));
        if !pattern.contains(['*', '?']) {
            if base.is_dir() {
                for file in self.files_below(&base, origin) {
                    if is_php(&file) {
                        self.add_canonical(file, origin);
                    }
                }
            } else {
                // A named file is taken whatever its extension: `bin/console`.
                self.add_referenced(&base, origin);
            }
            return;
        }
        // A pattern that spells an extension asks for it: `views/**/*.phtml`.
        let extension = glob::literal_extension(&pattern);
        for file in self.files_below(&base, origin) {
            let wanted = is_php(&file)
                || extension.is_some_and(|wanted| {
                    file.extension()
                        .is_some_and(|found| found.eq_ignore_ascii_case(wanted))
                });
            if wanted && glob::matches_path_or_parent(&pattern, &relative_path(self.root, &file)) {
                self.add_canonical(file, origin);
            }
        }
    }

    fn add_composer(
        &mut self,
        composer: &ComposerData,
        autoload_files: &mut Vec<PathBuf>,
        classmap: &mut BTreeMap<String, PathBuf>,
    ) {
        // The loader itself: `vendor/autoload.php` and the files beside its tables.
        for name in ["autoload.php", "autoload_runtime.php"] {
            let file = composer.vendor_dir.join(name);
            if file.is_file() {
                self.add_referenced(&file, Origin::Requested);
            }
        }
        let composer_dir = composer.vendor_dir.join("composer");
        for entry in std::fs::read_dir(&composer_dir)
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            if is_php(&path) && path.is_file() {
                self.add_referenced(&path, Origin::Requested);
            }
        }

        // Required on every request whether the package is a development
        // one or not: the loader's table lists them and it does not ask
        // what the build left out.
        for file in &composer.files {
            if let Some(path) = self.add_referenced(file, Origin::Requested) {
                self.files.entry(path.clone()).or_default().always_loaded = true;
                if !autoload_files.contains(&path) {
                    autoload_files.push(path);
                }
            }
        }

        // The class map answers before any PSR-4 prefix is tried.
        for (class, file) in &composer.classmap {
            if let Some(path) = self.add_referenced(file, Origin::Scanned) {
                classmap.entry(class.to_ascii_lowercase()).or_insert(path);
            }
        }

        // Composer tries the longest prefix first and takes the first file
        // it finds. The sort is stable: directories keep their order.
        let mut prefixes: Vec<&(String, Vec<PathBuf>)> = composer.psr4.iter().collect();
        prefixes.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
        for (prefix, directories) in prefixes {
            for directory in directories {
                let Ok(directory) = directory.canonicalize() else {
                    self.missing.insert(without_dot_components(directory));
                    continue;
                };
                for file in self.files_below(&directory, Origin::Scanned) {
                    if !is_php(&file) {
                        continue;
                    }
                    if let Some(class) = psr4_class(prefix, &directory, &file) {
                        classmap.entry(class).or_insert_with(|| file.clone());
                    }
                    self.files.entry(file).or_default();
                }
            }
        }

        for directory in &composer.psr0_directories {
            for file in self.files_below(directory, Origin::Scanned) {
                if is_php(&file) {
                    self.files.entry(file).or_default();
                }
            }
        }
    }
}

/// The lowercased class PSR-4 gives a file: `app/Models/User.php` under the
/// prefix `App\` is `app\models\user`.
fn psr4_class(prefix: &str, directory: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(directory).ok()?.to_str()?;
    // Composer appends exactly `.php`; `User.PHP` is never autoloaded.
    let class = relative.strip_suffix(".php")?;
    Some(format!("{prefix}{}", class.replace(['/', '\\'], "\\")).to_ascii_lowercase())
}

fn is_php(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("php"))
}

/// `vendor/composer/../acme/a.php` as `vendor/acme/a.php`, without asking
/// the file system: the path may not exist.
fn without_dot_components(path: &Path) -> PathBuf {
    let mut plain = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                plain.pop();
            }
            other => plain.push(other),
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::discover;
    use crate::sound::BuildConfig;
    use phera_bytecode::facts::BuildMode;
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    fn write(root: &Path, name: &str, contents: &str) {
        let file = root.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }

    fn config(root: &Path) -> BuildConfig {
        BuildConfig {
            project_root: root.to_path_buf(),
            mode: BuildMode::Hybrid,
            entries: Vec::new(),
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            include_dev: false,
            allow: Vec::new(),
            native_functions: BTreeSet::new(),
            native_classes: BTreeSet::new(),
            native_supertypes: std::collections::BTreeMap::new(),
        }
    }

    fn relative_paths(world: &crate::sound::DiscoveredWorld) -> Vec<&str> {
        world
            .files
            .iter()
            .map(|file| file.relative_path.as_str())
            .collect()
    }

    /// A small Composer project with one production and one development package.
    fn composer_project() -> (tempfile::TempDir, PathBuf) {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        write(&root, "public/index.php", "<?php");
        write(&root, "app/Models/User.php", "<?php");
        write(&root, "app/helpers.php", "<?php");
        write(&root, "app/notes.txt", "not PHP");
        write(&root, "tests/UserTest.php", "<?php");
        write(&root, "vendor/autoload.php", "<?php");
        write(&root, "vendor/composer/ClassLoader.php", "<?php");
        write(&root, "vendor/acme/lib/src/Client.php", "<?php");
        write(&root, "vendor/acme/lib/boot.php", "<?php");
        write(&root, "vendor/acme/tool/src/Tool.php", "<?php");
        write(&root, "vendor/acme/tool/functions.php", "<?php");
        write(
            &root,
            "composer.json",
            r#"{"autoload": {"psr-4": {"App\\": "app/"}},
                "autoload-dev": {"psr-4": {"Tests\\": "tests/"}}}"#,
        );
        write(
            &root,
            "vendor/composer/installed.json",
            r#"{"packages": [
                {"name": "acme/lib", "install-path": "../acme/lib"},
                {"name": "acme/tool", "install-path": "../acme/tool"}
            ], "dev-package-names": ["acme/tool"]}"#,
        );
        write(
            &root,
            "vendor/composer/autoload_files.php",
            r#"<?php
            $vendorDir = dirname(__DIR__);
            $baseDir = dirname($vendorDir);
            return array(
                't' => $vendorDir . '/acme/tool/functions.php',
                'b' => $vendorDir . '/acme/lib/boot.php',
                'h' => $baseDir . '/app/helpers.php',
                'gone' => $vendorDir . '/acme/lib/removed.php',
            );"#,
        );
        write(
            &root,
            "vendor/composer/autoload_classmap.php",
            r#"<?php
            $vendorDir = dirname(__DIR__);
            $baseDir = dirname($vendorDir);
            return array(
                'Acme\\Tool\\Tool' => $vendorDir . '/acme/tool/src/Tool.php',
                'Composer\\Autoload\\ClassLoader' => $vendorDir . '/composer/ClassLoader.php',
            );"#,
        );
        write(
            &root,
            "vendor/composer/autoload_psr4.php",
            r#"<?php
            $vendorDir = dirname(__DIR__);
            $baseDir = dirname($vendorDir);
            return array(
                'Tests\\' => array($baseDir . '/tests'),
                'App\\' => array($baseDir . '/app'),
                'Acme\\Lib\\' => array($vendorDir . '/acme/lib/src'),
                'Acme\\Tool\\' => array($vendorDir . '/acme/tool/src'),
            );"#,
        );
        (project, root)
    }

    #[test]
    fn a_composer_project_is_found_without_its_development_code() {
        let (_project, root) = composer_project();
        let world = discover(&config(&root)).unwrap();
        assert_eq!(world.project_root, root);
        assert_eq!(world.framework, None);
        assert_eq!(
            relative_paths(&world),
            [
                "app/Models/User.php",
                "app/helpers.php",
                "public/index.php",
                "vendor/acme/lib/boot.php",
                "vendor/acme/lib/src/Client.php",
                // A development package's autoload file is still required on every request.
                "vendor/acme/tool/functions.php",
                "vendor/autoload.php",
                "vendor/composer/ClassLoader.php",
                "vendor/composer/autoload_classmap.php",
                "vendor/composer/autoload_files.php",
                "vendor/composer/autoload_psr4.php",
            ]
        );
        // Composer's order, not the alphabet.
        assert_eq!(
            world.autoload_files,
            [
                root.join("vendor/acme/tool/functions.php"),
                root.join("vendor/acme/lib/boot.php"),
                root.join("app/helpers.php"),
            ]
        );
        assert_eq!(world.skipped_missing, 1);
        let entry = &world.files[2];
        assert!(entry.is_entry && !entry.always_loaded);
        let helpers = &world.files[1];
        assert!(helpers.always_loaded && !helpers.is_entry);
        assert_eq!(
            world.classmap.keys().collect::<Vec<_>>(),
            [
                "acme\\lib\\client",
                "app\\helpers",
                "app\\models\\user",
                "composer\\autoload\\classloader",
            ]
        );
        assert_eq!(
            world.classmap["app\\models\\user"],
            root.join("app/Models/User.php")
        );
    }

    #[test]
    fn development_code_joins_when_asked_for() {
        let (_project, root) = composer_project();
        let world = discover(&BuildConfig {
            include_dev: true,
            ..config(&root)
        })
        .unwrap();
        let paths = relative_paths(&world);
        assert!(paths.contains(&"tests/UserTest.php"));
        assert!(paths.contains(&"vendor/acme/tool/src/Tool.php"));
        assert!(world.classmap.contains_key("acme\\tool\\tool"));
        assert!(world.classmap.contains_key("tests\\usertest"));
    }

    #[test]
    fn exclusions_win_over_scans_and_inclusions_but_not_over_entries() {
        let (_project, root) = composer_project();
        write(&root, "legacy/old.php", "<?php");
        write(&root, "legacy/keep/new.php", "<?php");
        write(&root, "legacy/page.phtml", "<p>");
        write(&root, "bin/tool", "#!/usr/bin/env php\n<?php");
        let world = discover(&BuildConfig {
            include_globs: vec![
                "legacy/**/*.php".into(),
                "legacy/*.phtml".into(),
                "bin/tool".into(),
                "tests".into(),
                "nowhere/file.php".into(),
            ],
            exclude_globs: vec!["legacy/old.php".into(), "app".into(), "public".into()],
            ..config(&root)
        })
        .unwrap();
        let paths = relative_paths(&world);
        assert!(paths.contains(&"legacy/keep/new.php"));
        assert!(paths.contains(&"legacy/page.phtml"));
        assert!(paths.contains(&"bin/tool"));
        // Asked for by name, so being development code does not remove it.
        assert!(paths.contains(&"tests/UserTest.php"));
        assert!(!paths.contains(&"legacy/old.php"));
        assert!(!paths.contains(&"app/Models/User.php"));
        assert!(!world.classmap.contains_key("app\\models\\user"));
        // The entry script was not found by a scan: an exclusion keeps it.
        assert!(paths.contains(&"public/index.php"));
        // One file Composer names and one the configuration names are missing.
        assert_eq!(world.skipped_missing, 2);
    }

    #[test]
    fn explicit_entries_replace_the_detected_ones_and_must_exist() {
        let (_project, root) = composer_project();
        write(&root, "worker.php", "<?php");
        let world = discover(&BuildConfig {
            entries: vec!["worker.php".into()],
            ..config(&root)
        })
        .unwrap();
        let entries: Vec<&str> = world
            .files
            .iter()
            .filter(|file| file.is_entry)
            .map(|file| file.relative_path.as_str())
            .collect();
        assert_eq!(entries, ["worker.php"]);
        assert!(!relative_paths(&world).contains(&"public/index.php"));

        let error = discover(&BuildConfig {
            entries: vec!["missing.php".into()],
            ..config(&root)
        })
        .unwrap_err();
        assert!(error.contains("missing.php"), "{error}");
    }

    #[test]
    fn a_laravel_project_brings_its_dynamically_loaded_files() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        for name in [
            "artisan",
            "public/index.php",
            "bootstrap/app.php",
            "bootstrap/cache/services.php",
            "config/app.php",
            "config/nested/deep.php",
            "routes/web.php",
            "storage/framework/views/abc.php",
            "storage/logs/dump.php",
            "lang/en/auth.php",
            "resources/views/welcome.blade.php",
        ] {
            write(&root, name, "<?php");
        }
        let world = discover(&config(&root)).unwrap();
        assert_eq!(world.framework, Some("laravel"));
        assert_eq!(
            relative_paths(&world),
            [
                "artisan",
                "bootstrap/app.php",
                "bootstrap/cache/services.php",
                "config/app.php",
                "config/nested/deep.php",
                "lang/en/auth.php",
                "public/index.php",
                "routes/web.php",
                "storage/framework/views/abc.php",
            ]
        );
        let entries: Vec<&str> = world
            .files
            .iter()
            .filter(|file| file.is_entry)
            .map(|file| file.relative_path.as_str())
            .collect();
        assert_eq!(entries, ["artisan", "public/index.php"]);
    }

    #[test]
    fn a_plain_project_is_its_entry_script() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        write(&root, "index.php", "<?php");
        write(&root, "lib/a.php", "<?php");
        let world = discover(&config(&root)).unwrap();
        assert_eq!(relative_paths(&world), ["index.php"]);
        assert!(world.autoload_files.is_empty());
        assert!(world.classmap.is_empty());
    }

    #[test]
    fn a_missing_project_root_is_an_error() {
        let project = tempfile::tempdir().unwrap();
        let error = discover(&config(&project.path().join("nope"))).unwrap_err();
        assert!(error.contains("nope"), "{error}");
    }
}
