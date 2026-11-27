//! Composer's generated autoload data, read as data and never executed:
//!
//! ```php
//! // vendor/composer/autoload_psr4.php
//! $vendorDir = dirname(__DIR__);
//! $baseDir = dirname($vendorDir);
//! return array('App\\' => array($baseDir . '/app'));
//! ```
//!
//! Only strings, `.`, `__DIR__`, `__FILE__`, `dirname()`, the two directory
//! variables and arrays are understood. An entry written any other way is
//! left out, so a hand-edited file cannot make the build run code.

use std::path::{Path, PathBuf};

use phera_ast::{
    Arguments, BinaryOperator, ClassMemberKind, Expression, ExpressionKind, Literal, MagicConstant,
    Program, Statement, StatementKind, Variable,
};
use serde_json::Value;

/// What Composer's generated files say about a project. Paths are as the
/// files spell them (`vendor/composer/../acme/a.php`), existing or not.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ComposerData {
    pub(crate) vendor_dir: PathBuf,
    /// `autoload.files`, in the order Composer's loader requires them.
    pub(crate) files: Vec<PathBuf>,
    /// A class name as written, and the file that declares it.
    pub(crate) classmap: Vec<(String, PathBuf)>,
    /// A PSR-4 prefix (`App\`, or empty for the fallback directories) and
    /// its directories in lookup order. A prefix can appear twice.
    pub(crate) psr4: Vec<(String, Vec<PathBuf>)>,
    /// PSR-0 directories: their files join the build without class names.
    pub(crate) psr0_directories: Vec<PathBuf>,
    /// Canonical directories and files of `require-dev` packages and of the
    /// project's own `autoload-dev`.
    pub(crate) dev_paths: Vec<PathBuf>,
}

/// Reads `vendor/composer` of the project at `root`; `None` without one.
pub(crate) fn read(root: &Path) -> Result<Option<ComposerData>, String> {
    let manifest = read_json(&root.join("composer.json"))?;
    let vendor_dir = root.join(vendor_directory(manifest.as_ref()));
    let composer_dir = vendor_dir.join("composer");
    if !composer_dir.is_dir() {
        return Ok(None);
    }
    let mut data = ComposerData {
        vendor_dir,
        ..ComposerData::default()
    };
    // The loader reads the static file; the four others are its fallback
    // and say the same. Reading both costs little and misses nothing.
    let static_file = composer_dir.join("autoload_static.php");
    if let Some(program) = parse(&static_file)? {
        for (property, value) in static_properties(&program, &static_file) {
            data.add(&property, &value);
        }
    }
    let returned_files = [
        ("autoload_files.php", "files"),
        ("autoload_classmap.php", "classMap"),
        ("autoload_psr4.php", "prefixDirsPsr4"),
        ("autoload_namespaces.php", "prefixesPsr0"),
    ];
    for (name, property) in returned_files {
        let file = composer_dir.join(name);
        if let Some(program) = parse(&file)?
            && let Some(value) = returned_value(&program, &file)
        {
            data.add(property, &value);
        }
    }
    data.dev_paths = dev_paths(root, &composer_dir, manifest.as_ref())?;
    Ok(Some(data))
}

impl ComposerData {
    /// Takes one of the loader's tables, named as in `autoload_static.php`.
    fn add(&mut self, property: &str, value: &Data) {
        match property {
            "files" => value.paths_into(&mut self.files),
            "classMap" => {
                for (class, file) in value.entries() {
                    if let (Some(class), Some(file)) = (class, file.text()) {
                        self.classmap.push((class.clone(), PathBuf::from(file)));
                    }
                }
            }
            "prefixDirsPsr4" => {
                for (prefix, directories) in value.entries() {
                    if let Some(prefix) = prefix {
                        let mut paths = Vec::new();
                        directories.paths_into(&mut paths);
                        self.psr4.push((prefix.clone(), paths));
                    }
                }
            }
            "fallbackDirsPsr4" => {
                let mut paths = Vec::new();
                value.paths_into(&mut paths);
                self.psr4.push((String::new(), paths));
            }
            "prefixesPsr0" | "fallbackDirsPsr0" => value.paths_into(&mut self.psr0_directories),
            _ => {}
        }
    }
}

/// A value in Composer's data: a path, or an array of values.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Data {
    Text(String),
    /// The key is `None` for a list element and for an integer key.
    List(Vec<(Option<String>, Data)>),
}

impl Data {
    fn text(&self) -> Option<&str> {
        match self {
            Data::Text(text) => Some(text),
            Data::List(_) => None,
        }
    }

    fn entries(&self) -> &[(Option<String>, Data)] {
        match self {
            Data::Text(_) => &[],
            Data::List(entries) => entries,
        }
    }

    /// Every path below this value, in source order, whatever the nesting.
    fn paths_into(&self, paths: &mut Vec<PathBuf>) {
        match self {
            Data::Text(text) => paths.push(PathBuf::from(text)),
            Data::List(entries) => {
                for (_, value) in entries {
                    value.paths_into(paths);
                }
            }
        }
    }
}

/// What the expressions of one data file can refer to.
struct Scope<'a> {
    /// The data file: what `__FILE__` and `__DIR__` mean inside it.
    file: &'a Path,
    /// `$vendorDir` and `$baseDir`, as the file assigned them.
    variables: Vec<(String, String)>,
}

impl Scope<'_> {
    fn evaluate(&self, expression: &Expression) -> Option<Data> {
        match &expression.kind {
            ExpressionKind::Literal(Literal::String(bytes)) => {
                String::from_utf8(bytes.clone()).ok().map(Data::Text)
            }
            ExpressionKind::Binary {
                operator: BinaryOperator::Concat,
                left,
                right,
            } => match (self.evaluate(left)?, self.evaluate(right)?) {
                (Data::Text(left), Data::Text(right)) => Some(Data::Text(left + &right)),
                _ => None,
            },
            ExpressionKind::MagicConstant(MagicConstant::File) => Some(path_text(self.file)),
            ExpressionKind::MagicConstant(MagicConstant::Dir) => self.file.parent().map(path_text),
            ExpressionKind::Variable(Variable::Named(name)) => self
                .variables
                .iter()
                .find(|(known, _)| known == name)
                .map(|(_, value)| Data::Text(value.clone())),
            ExpressionKind::Call {
                callee,
                arguments: Arguments::List(arguments),
                ..
            } => {
                let ExpressionKind::Name(name) = &callee.kind else {
                    return None;
                };
                let [argument] = arguments.as_slice() else {
                    return None;
                };
                if !name.text.eq_ignore_ascii_case("dirname")
                    || argument.spread
                    || argument.name.is_some()
                {
                    return None;
                }
                let Data::Text(path) = self.evaluate(&argument.value)? else {
                    return None;
                };
                Path::new(&path).parent().map(path_text)
            }
            ExpressionKind::Array(items) => Some(Data::List(
                items
                    .iter()
                    .filter(|item| !item.spread && !item.by_reference)
                    .filter_map(|item| {
                        let key = item
                            .key
                            .as_ref()
                            .and_then(|key| self.evaluate(key))
                            .and_then(|key| key.text().map(str::to_owned));
                        Some((key, self.evaluate(&item.value)?))
                    })
                    .collect(),
            )),
            _ => None,
        }
    }
}

fn path_text(path: &Path) -> Data {
    Data::Text(path.to_string_lossy().into_owned())
}

/// The array a file such as `autoload_classmap.php` returns, after its
/// `$vendorDir = ...; $baseDir = ...;` assignments.
fn returned_value(program: &Program, file: &Path) -> Option<Data> {
    let mut scope = Scope {
        file,
        variables: Vec::new(),
    };
    for statement in &program.statements {
        match &statement.kind {
            StatementKind::Expression(expression) => {
                if let ExpressionKind::Assign { target, value } = &expression.kind
                    && let ExpressionKind::Variable(Variable::Named(name)) = &target.kind
                    && let Some(Data::Text(text)) = scope.evaluate(value)
                {
                    scope.variables.push((name.clone(), text));
                }
            }
            StatementKind::Return(Some(value)) => return scope.evaluate(value),
            _ => {}
        }
    }
    None
}

/// The defaults of the static properties in `autoload_static.php`:
/// `public static $files = array(...)`.
fn static_properties(program: &Program, file: &Path) -> Vec<(String, Data)> {
    let scope = Scope {
        file,
        variables: Vec::new(),
    };
    let mut properties = Vec::new();
    collect_static_properties(&program.statements, &scope, &mut properties);
    properties
}

fn collect_static_properties(
    statements: &[Statement],
    scope: &Scope<'_>,
    properties: &mut Vec<(String, Data)>,
) {
    for statement in statements {
        match &statement.kind {
            StatementKind::Namespace { statements, .. } => {
                collect_static_properties(statements, scope, properties);
            }
            StatementKind::Class(class) => {
                for member in &class.members {
                    let ClassMemberKind::Property { items, .. } = &member.kind else {
                        continue;
                    };
                    for item in items {
                        if let Some(default) = &item.default
                            && let Some(value) = scope.evaluate(default)
                        {
                            properties.push((item.name.clone(), value));
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

/// The syntax tree of a data file; `None` when the file does not exist.
fn parse(file: &Path) -> Result<Option<Program>, String> {
    let source = match std::fs::read(file) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", file.display())),
    };
    phera_parser::parse_bytes(&source)
        .map(Some)
        .map_err(|error| format!("{}: {}", file.display(), error.message))
}

fn read_json(file: &Path) -> Result<Option<Value>, String> {
    let source = match std::fs::read(file) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", file.display())),
    };
    serde_json::from_slice(&source)
        .map(Some)
        .map_err(|error| format!("{}: {error}", file.display()))
}

/// `config.vendor-dir` of composer.json, or Composer's default.
fn vendor_directory(manifest: Option<&Value>) -> &str {
    manifest
        .and_then(|manifest| manifest.get("config"))
        .and_then(|config| config.get("vendor-dir"))
        .and_then(Value::as_str)
        .unwrap_or("vendor")
}

/// Where development-only code lives: the project's `autoload-dev` paths
/// and the install directories of the packages Composer lists as
/// `dev-package-names`. Paths that do not exist are left out.
fn dev_paths(
    root: &Path,
    composer_dir: &Path,
    manifest: Option<&Value>,
) -> Result<Vec<PathBuf>, String> {
    let mut paths = Vec::new();
    if let Some(autoload_dev) = manifest.and_then(|manifest| manifest.get("autoload-dev")) {
        for section in ["psr-4", "psr-0"] {
            let directories = autoload_dev.get(section).and_then(Value::as_object);
            for directory in directories
                .into_iter()
                .flatten()
                .flat_map(|(_, v)| strings(v))
            {
                paths.push(root.join(directory));
            }
        }
        for section in ["classmap", "files"] {
            for path in autoload_dev.get(section).into_iter().flat_map(strings) {
                paths.push(root.join(path));
            }
        }
    }
    if let Some(installed) = read_json(&composer_dir.join("installed.json"))? {
        let dev_names: Vec<&str> = installed
            .get("dev-package-names")
            .into_iter()
            .flat_map(strings)
            .collect();
        let packages = installed.get("packages").and_then(Value::as_array);
        for package in packages.into_iter().flatten() {
            let Some(name) = package.get("name").and_then(Value::as_str) else {
                continue;
            };
            if !dev_names.contains(&name) {
                continue;
            }
            // `install-path` is relative to vendor/composer: "../acme/tool".
            let directory = match package.get("install-path").and_then(Value::as_str) {
                Some(install_path) => composer_dir.join(install_path),
                None => composer_dir.join("..").join(name),
            };
            paths.push(directory);
        }
    }
    Ok(paths
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .collect())
}

/// A JSON string, or the strings of a JSON array: `"src/"` or `["src/", "lib/"]`.
fn strings(value: &Value) -> Vec<&str> {
    match value {
        Value::String(text) => vec![text.as_str()],
        Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::read;
    use std::path::{Path, PathBuf};

    fn write(root: &Path, name: &str, contents: &str) {
        let file = root.join(name);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, contents).unwrap();
    }

    fn canonical(paths: &[PathBuf]) -> Vec<PathBuf> {
        paths
            .iter()
            .map(|path| path.canonicalize().unwrap())
            .collect()
    }

    #[test]
    fn a_project_without_vendor_composer_has_no_data() {
        let project = tempfile::tempdir().unwrap();
        assert_eq!(read(project.path()).unwrap(), None);
    }

    #[test]
    fn static_and_returned_tables_are_read_in_order() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        write(&root, "app/helpers.php", "<?php");
        write(&root, "app/User.php", "<?php");
        write(&root, "vendor/acme/a.php", "<?php");
        write(&root, "vendor/acme/b.php", "<?php");
        write(
            &root,
            "vendor/composer/autoload_static.php",
            r#"<?php
            namespace Composer\Autoload;
            class ComposerStaticInit1
            {
                public static $files = array (
                    'b1' => __DIR__ . '/..' . '/acme/b.php',
                    'a1' => __DIR__ . '/..' . '/acme/a.php',
                    'h1' => __DIR__ . '/../..' . '/app/helpers.php',
                );
                public static $prefixLengthsPsr4 = array ('A' => array ('App\\' => 4));
                public static $prefixDirsPsr4 = array (
                    'App\\' => array (0 => __DIR__ . '/../..' . '/app'),
                );
                public static $classMap = array (
                    'App\\User' => __DIR__ . '/../..' . '/app/User.php',
                    'Evil' => some_function() . '/x.php',
                );
                public static function getInitializer(ClassLoader $loader) { return 1; }
            }"#,
        );
        write(
            &root,
            "vendor/composer/autoload_psr4.php",
            r#"<?php
            $vendorDir = dirname(__DIR__);
            $baseDir = dirname($vendorDir);
            return array(
                'Acme\\' => array($vendorDir . '/acme'),
                '' => array($baseDir . '/lib'),
            );"#,
        );
        write(
            &root,
            "vendor/composer/autoload_files.php",
            r#"<?php
            $vendorDir = dirname(dirname(__FILE__));
            $baseDir = dirname($vendorDir);
            return array('b1' => $vendorDir . '/acme/b.php');"#,
        );

        let data = read(&root).unwrap().unwrap();
        assert_eq!(data.vendor_dir, root.join("vendor"));
        // The static file's three files in its order, then the returned one.
        assert_eq!(
            canonical(&data.files),
            [
                root.join("vendor/acme/b.php"),
                root.join("vendor/acme/a.php"),
                root.join("app/helpers.php"),
                root.join("vendor/acme/b.php"),
            ]
        );
        // The entry that calls a function is not data and is left out.
        assert_eq!(data.classmap.len(), 1);
        assert_eq!(data.classmap[0].0, "App\\User");
        assert_eq!(
            data.classmap[0].1.canonicalize().unwrap(),
            root.join("app/User.php")
        );
        let prefixes: Vec<&str> = data.psr4.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(prefixes, ["App\\", "Acme\\", ""]);
        assert_eq!(data.psr4[1].1, [root.join("vendor/acme")]);
        assert_eq!(data.psr4[2].1, [root.join("lib")]);
    }

    #[test]
    fn dev_packages_and_autoload_dev_directories_are_listed() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        write(&root, "tests/UserTest.php", "<?php");
        write(&root, "vendor/acme/tool/src/Tool.php", "<?php");
        write(&root, "vendor/acme/lib/src/Lib.php", "<?php");
        write(
            &root,
            "composer.json",
            r#"{"autoload-dev": {"psr-4": {"Tests\\": "tests/", "Gone\\": ["missing/"]}}}"#,
        );
        write(
            &root,
            "vendor/composer/installed.json",
            r#"{"packages": [
                {"name": "acme/tool", "install-path": "../acme/tool"},
                {"name": "acme/lib", "install-path": "../acme/lib"}
            ], "dev": true, "dev-package-names": ["acme/tool"]}"#,
        );
        let data = read(&root).unwrap().unwrap();
        assert_eq!(
            data.dev_paths,
            [root.join("tests"), root.join("vendor/acme/tool")]
        );
    }

    #[test]
    fn a_custom_vendor_directory_is_followed() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        write(
            &root,
            "composer.json",
            r#"{"config": {"vendor-dir": "deps"}}"#,
        );
        write(
            &root,
            "deps/composer/autoload_files.php",
            "<?php $vendorDir = dirname(__DIR__); return array('a' => $vendorDir . '/a.php');",
        );
        let data = read(&root).unwrap().unwrap();
        assert_eq!(data.vendor_dir, root.join("deps"));
        assert_eq!(data.files, [root.join("deps/a.php")]);
    }

    #[test]
    fn a_data_file_that_does_not_parse_is_an_error() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path().canonicalize().unwrap();
        write(
            &root,
            "vendor/composer/autoload_classmap.php",
            "<?php return array(",
        );
        let error = read(&root).unwrap_err();
        assert!(error.contains("autoload_classmap.php"), "{error}");
    }
}
