//! Checker settings: `[check] level = 1` and `paths = ["src"]` in `phera.toml`.
//! Only the documented check table is interpreted; other tables are left alone.
//! For example, `new Missing` is checked at the configured level for these paths.

use crate::diagnostic::Format;
use std::{collections::BTreeSet, path::Path};

mod phpstan;
pub use phpstan::Ignore;

#[derive(Clone)]
pub struct Options {
    pub level: u8,
    pub paths: Vec<String>,
    pub scan: Vec<String>,
    pub exclude: Vec<String>,
    pub baseline: Option<String>,
    pub generate_baseline: Option<String>,
    pub format: Format,
    pub no_progress: bool,
    pub strict: bool,
    pub report_always_true_last_condition: bool,
    pub report_unmatched_ignores: bool,
    pub treat_phpdoc_types_as_certain: bool,
    pub check_missing_typehints: Option<bool>,
    pub check_phpdoc_missing_return: Option<bool>,
    pub unnecessary_null_coalesce: bool,
    pub internal_tag: bool,
    pub unused_parameters: bool,
    pub unused_variables: bool,
    pub symfony_stubs: bool,
    pub doctrine_stubs: bool,
    pub config_notices: BTreeSet<String>,
    pub phpstan_ignores: Vec<Ignore>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            level: 1,
            paths: Vec::new(),
            exclude: Vec::new(),
            scan: Vec::new(),
            baseline: None,
            generate_baseline: None,
            format: Format::Text,
            no_progress: false,
            strict: false,
            report_always_true_last_condition: false,
            report_unmatched_ignores: true,
            treat_phpdoc_types_as_certain: true,
            check_missing_typehints: None,
            check_phpdoc_missing_return: None,
            unnecessary_null_coalesce: false,
            internal_tag: false,
            unused_parameters: false,
            unused_variables: false,
            symfony_stubs: false,
            doctrine_stubs: false,
            config_notices: BTreeSet::new(),
            phpstan_ignores: Vec::new(),
        }
    }
}

impl Options {
    pub fn parse(arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        Self::parse_at(Path::new("."), arguments)
    }

    fn parse_at(directory: &Path, arguments: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut options = Self::default();
        phpstan::read(directory, &mut options)?;
        match std::fs::read_to_string(directory.join("phera.toml")) {
            Ok(source) => options.read_config(&source)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("phera.toml: {e}")),
        }
        let mut arguments = arguments.peekable();
        let mut paths = Vec::new();
        let mut use_phpstan_baseline = false;
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--" => {
                    paths.extend(arguments);
                    break;
                }
                "--level" => {
                    options.level = level(&arguments.next().ok_or("--level needs a value")?)?
                }
                "--format" => {
                    options.format = match arguments.next().as_deref() {
                        Some("text") => Format::Text,
                        Some("json") => Format::Json,
                        Some("github") => Format::Github,
                        _ => return Err("--format needs text, json or github".into()),
                    }
                }
                "--no-progress" => options.no_progress = true,
                "--strict" => options.strict = true,
                "--no-strict" => options.strict = false,
                "--baseline" => use_phpstan_baseline = true,
                "--generate-baseline" => {
                    options.generate_baseline =
                        Some(arguments.next().ok_or("--generate-baseline needs a file")?)
                }
                value if value.starts_with('-') => {
                    return Err(format!("Unknown check option: {value}"));
                }
                _ => paths.push(argument),
            }
        }
        if !paths.is_empty() {
            options.paths = paths;
        }
        if options.paths.is_empty() {
            let mut message =
                String::from("check needs paths, [check].paths or PHPStan parameters.paths");
            for notice in &options.config_notices {
                message.push('\n');
                message.push_str(notice);
            }
            return Err(message);
        }
        if use_phpstan_baseline && options.baseline.is_none() {
            options.baseline = Some(default_baseline(&options.paths));
        }
        Ok(options)
    }

    fn read_config(&mut self, source: &str) -> Result<(), String> {
        read_tables(
            source,
            |table| table == "check",
            |_, key, values, array| self.setting(key, values, array),
        )
    }

    fn setting(&mut self, key: &str, values: Vec<String>, array: bool) -> Result<(), String> {
        let scalar = || {
            values
                .first()
                .ok_or_else(|| format!("phera.toml: missing {key}"))
        };
        match key {
            "level" if !array => self.level = level(scalar()?)?,
            "strict" if !array => self.strict = boolean("strict", scalar()?)?,
            "internal_tag" if !array => self.internal_tag = boolean(key, scalar()?)?,
            "unused_parameters" if !array => self.unused_parameters = boolean(key, scalar()?)?,
            "unused_variables" if !array => self.unused_variables = boolean(key, scalar()?)?,
            "treat_phpdoc_types_as_certain" if !array => {
                self.treat_phpdoc_types_as_certain = boolean(key, scalar()?)?;
            }
            "report_unmatched_ignores" if !array => {
                self.report_unmatched_ignores = boolean(key, scalar()?)?;
            }
            "report_always_true_last_condition" if !array => {
                self.report_always_true_last_condition = boolean(key, scalar()?)?;
            }
            "check_missing_typehints" if !array => {
                self.check_missing_typehints = Some(boolean(key, scalar()?)?);
            }
            "check_phpdoc_missing_return" if !array => {
                self.check_phpdoc_missing_return = Some(boolean(key, scalar()?)?);
            }
            "paths" if array => {
                self.paths = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
            }
            "scan" if array => {
                self.scan = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
            }
            "exclude" if array => {
                self.exclude = values.iter().map(|s| string(s)).collect::<Result<_, _>>()?
            }
            "baseline" if !array => self.baseline = Some(string(scalar()?)?),
            "format" if !array => {
                self.format = match string(scalar()?)?.as_str() {
                    "text" => Format::Text,
                    "json" => Format::Json,
                    "github" => Format::Github,
                    _ => return Err("phera.toml: format must be text, json or github".into()),
                }
            }
            "no_progress" | "no-progress" if !array => {
                self.no_progress = boolean("no_progress", scalar()?)?;
            }
            _ => {
                return Err(format!(
                    "phera.toml: unsupported check setting or value: {key}"
                ));
            }
        }
        Ok(())
    }
}

fn boolean(key: &str, value: &str) -> Result<bool, String> {
    value
        .parse()
        .map_err(|_| format!("phera.toml: {key} must be true or false"))
}

fn default_baseline(paths: &[String]) -> String {
    const FILE: &str = "phpstan-baseline.neon";
    if std::path::Path::new(FILE).is_file() {
        return FILE.into();
    }
    paths
        .iter()
        .filter_map(|path| std::path::Path::new(path).canonicalize().ok())
        .find_map(|path| {
            path.ancestors()
                .map(|directory| directory.join(FILE))
                .find(|file| file.is_file())
        })
        .map_or_else(|| FILE.into(), |file| file.to_string_lossy().into_owned())
}

/// Read the requested tables from settings shared by the analyzer commands.
pub(crate) fn read_tables(
    source: &str,
    interested: impl Fn(&str) -> bool,
    mut setting: impl FnMut(&str, &str, Vec<String>, bool) -> Result<(), String>,
) -> Result<(), String> {
    let tokens = tokenize(source)?;
    let mut table = String::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        if tokens[cursor] == "[" {
            cursor += 1;
            table = tokens.get(cursor).cloned().unwrap_or_default();
            while cursor < tokens.len() && tokens[cursor] != "]" {
                cursor += 1;
            }
            cursor += 1;
            continue;
        }
        let key = tokens[cursor].clone();
        cursor += 1;
        if tokens.get(cursor).is_none_or(|s| s != "=") {
            if interested(&table) {
                return Err(format!("phera.toml: expected '=' after {key}"));
            }
            continue;
        }
        cursor += 1;
        let mut values = Vec::new();
        let array = tokens.get(cursor).is_some_and(|s| s == "[");
        if array {
            cursor += 1;
            while cursor < tokens.len() && tokens[cursor] != "]" {
                if tokens[cursor] != "," {
                    values.push(tokens[cursor].clone());
                }
                cursor += 1;
            }
            if cursor == tokens.len() {
                return Err("phera.toml: unclosed array".into());
            }
            cursor += 1;
        } else if let Some(value) = tokens.get(cursor) {
            values.push(value.clone());
            cursor += 1;
        }
        if !interested(&table) {
            continue;
        }
        setting(&table, &key, values, array)?;
    }
    Ok(())
}

fn level(value: &str) -> Result<u8, String> {
    match value {
        "0" => Ok(0),
        "1" => Ok(1),
        "2" => Ok(2),
        "3" => Ok(3),
        "4" => Ok(4),
        "5" => Ok(5),
        "6" => Ok(6),
        "7" => Ok(7),
        "8" => Ok(8),
        "9" => Ok(9),
        "10" | "max" => Ok(10),
        _ => Err("Supported check levels are 0 through 10, or max".into()),
    }
}

pub(crate) fn string(value: &str) -> Result<String, String> {
    value
        .strip_prefix('"')
        .map(String::from)
        .ok_or("phera.toml: expected a string".into())
}

fn tokenize(source: &str) -> Result<Vec<String>, String> {
    let mut input = source.chars().peekable();
    let mut tokens = Vec::new();
    while let Some(c) = input.next() {
        match c {
            c if c.is_whitespace() => {}
            '#' => {
                while input.peek().is_some_and(|c| *c != '\n') {
                    input.next();
                }
            }
            '[' | ']' | '=' | ',' => tokens.push(c.to_string()),
            '"' | '\'' => {
                let mut value = String::from("\"");
                let mut closed = false;
                while let Some(next) = input.next() {
                    if next == c {
                        closed = true;
                        break;
                    }
                    if next == '\\' && c == '"' {
                        value.push(match input.next().ok_or("phera.toml: unfinished escape")? {
                            'n' => '\n',
                            'r' => '\r',
                            't' => '\t',
                            '\\' => '\\',
                            '"' => '"',
                            _ => return Err("phera.toml: unsupported string escape".into()),
                        });
                    } else {
                        value.push(next);
                    }
                }
                if !closed {
                    return Err("phera.toml: unclosed string".into());
                }
                tokens.push(value);
            }
            _ => {
                let mut value = c.to_string();
                while input.peek().is_some_and(|c| {
                    !c.is_whitespace() && !matches!(c, '#' | '[' | ']' | '=' | ',')
                }) {
                    if let Some(c) = input.next() {
                        value.push(c);
                    }
                }
                tokens.push(value);
            }
        }
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn checker_configuration_uses_project_precedence() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        fs::create_dir(root.join("config")).unwrap();
        fs::write(
            root.join("phpstan.dist.neon"),
            "parameters:\n    level: 10\n    paths: [wrong-dist]\n",
        )
        .unwrap();
        fs::write(
            root.join("phpstan.neon.dist"),
            "parameters:\n    level: 9\n    paths: [wrong-neon-dist]\n",
        )
        .unwrap();
        fs::write(
            root.join("config/local.neon"),
            "parameters:\n    level: 7\n    paths: [source]\n",
        )
        .unwrap();
        fs::write(
            root.join("phpstan.neon"),
            r#"
includes:
    - config/local.neon
    - vendor/phpstan/phpstan-strict-rules/rules.neon
    - vendor/sample/custom/extension.neon
    - vendor/sample/custom/rules.neon
parameters:
    level: 8
    paths: ['tests, new']
    treatPhpDocTypesAsCertain: false
    checkMissingTypehints: false
    excludePaths:
        analyse:
            - src/skip.php
"#,
        )
        .unwrap();

        let loaded = Options::parse_at(root, std::iter::empty()).unwrap();
        assert!(loaded.strict);
        assert!(!loaded.treat_phpdoc_types_as_certain);
        assert_eq!(loaded.check_missing_typehints, Some(false));
        assert_eq!(loaded.level, 8);
        assert_eq!(
            loaded.paths[0],
            root.join("config/source").to_string_lossy()
        );
        assert_eq!(loaded.paths[1], root.join("tests, new").to_string_lossy());
        assert_eq!(
            loaded.exclude,
            [root.join("src/skip.php").to_string_lossy()]
        );
        assert_eq!(
            loaded.config_notices.into_iter().collect::<Vec<_>>(),
            ["PHPStan configuration sample/custom: not applied"],
        );

        fs::write(
            root.join("phera.toml"),
            "[check]\nstrict = false\nlevel = 4\n",
        )
        .unwrap();
        let local = Options::parse_at(root, std::iter::empty()).unwrap();
        assert!(!local.strict);
        assert_eq!(local.level, 4);

        let command = ["--level", "1", "--strict", "src"]
            .into_iter()
            .map(String::from);
        let command = Options::parse_at(root, command).unwrap();
        assert!(command.strict);
        assert_eq!(command.level, 1);
        assert_eq!(command.paths, ["src"]);
    }

    #[test]
    fn phpstan_applies_package_includes_in_configuration_order() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        fs::write(
            root.join("local.neon"),
            "parameters:\n    featureToggles:\n        unusedVariable: false\n",
        )
        .unwrap();
        let package = "vendor/phpstan/phpstan/conf/bleedingEdge.neon";
        for (first, second, enabled) in [
            ("local.neon", package, true),
            (package, "local.neon", false),
        ] {
            fs::write(
                root.join("phpstan.neon"),
                format!(
                    "includes:\n    - {first}\n    - {second}\nparameters:\n    paths: [src]\n"
                ),
            )
            .unwrap();
            let options = Options::parse_at(root, std::iter::empty()).unwrap();
            assert_eq!(options.unused_variables, enabled);
        }
    }

    #[test]
    fn phpstan_ignore_counts_apply_by_identifier_and_path() {
        let project = tempfile::tempdir().unwrap();
        let root = project.path();
        fs::write(
            root.join("src.php"),
            "<?php\nfunction test(): void {\n    missing_one();\n    missing_two();\n}\n",
        )
        .unwrap();
        fs::write(
            root.join("phpstan.neon"),
            r#"
parameters:
    level: 0
    paths:
        - src.php
    ignoreErrors:
        - identifier: function.notFound
          message: '#missing_(?=one|two)#'
          path: src.php
          count: 1
"#,
        )
        .unwrap();
        let options = Options::parse_at(root, std::iter::empty()).unwrap();
        assert!(options.config_notices.is_empty());
        let report = crate::check(&options).unwrap();
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].line, 4);
        assert_eq!(report.diagnostics[0].identifier, "function.notFound");
    }
}
