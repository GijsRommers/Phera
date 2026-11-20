//! Read settings such as `level: 8` under the PHPStan NEON `parameters:` key.
//! Includes are inspected as data. Composer extensions and PHP files never run.

use super::{Options, level};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod ignores;
mod lines;
pub use ignores::Ignore;
use lines::{blocks, key_value, list, scalar};

const CONFIG_NAMES: &[&str] = &["phpstan.neon", "phpstan.neon.dist", "phpstan.dist.neon"];

enum Visit {
    Read(PathBuf),
    Apply(PathBuf, String),
}

pub(super) fn read(directory: &Path, options: &mut Options) -> Result<(), String> {
    let Some(config) = CONFIG_NAMES
        .iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
    else {
        if directory.join("phpstan.php").is_file() {
            notice(options, "phpstan.php");
        }
        return Ok(());
    };

    let mut visited = BTreeSet::new();
    let mut pending = vec![Visit::Read(config)];
    while let Some(visit) = pending.pop() {
        match visit {
            Visit::Read(path) => {
                let canonical = path
                    .canonicalize()
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                if !visited.insert(canonical) {
                    continue;
                }
                let source = std::fs::read_to_string(&path)
                    .map_err(|e| format!("{}: {e}", path.display()))?;
                let records = lines::read(&source);
                let includes = blocks(&records)
                    .filter_map(|(line, children)| {
                        let (key, value) = key_value(line)?;
                        (key == "includes").then(|| list(value, children))
                    })
                    .flatten()
                    .collect::<Vec<_>>();
                let parent = path.parent().unwrap_or(directory);
                let includes = includes
                    .into_iter()
                    .filter_map(|include| include_visit(&include, directory, parent, options))
                    .collect::<Vec<_>>();
                pending.push(Visit::Apply(path, source));
                pending.extend(includes.into_iter().rev());
            }
            Visit::Apply(path, source) => {
                let records = lines::read(&source);
                let parent = path.parent().unwrap_or(directory);
                for (line, children) in blocks(&records) {
                    if key_value(line) == Some(("parameters", "")) {
                        parameters(children, directory, parent, options)?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn include_visit(
    include: &str,
    directory: &Path,
    parent: &Path,
    options: &mut Options,
) -> Option<Visit> {
    let path = Path::new(include);
    if let Some(package) = composer_package(path) {
        match package.as_str() {
            "phpstan/phpstan-strict-rules" if path.ends_with("rules.neon") => options.strict = true,
            "phpstan/phpstan-doctrine" if path.ends_with("extension.neon") => {
                options.doctrine_stubs = true;
            }
            "phpstan/phpstan-doctrine" => {}
            "phpstan/phpstan-symfony" if path.ends_with("extension.neon") => {
                options.symfony_stubs = true;
            }
            "phpstan/phpstan-symfony" => {}
            "phpstan/phpstan" if path.ends_with("bleedingEdge.neon") => {
                options.unnecessary_null_coalesce = true;
                options.internal_tag = true;
                notice(options, "remaining bleeding-edge features");
            }
            _ => notice(options, &package),
        }
        return None;
    }
    let filename = resolve(include, directory, parent);
    if filename.extension().is_some_and(|ext| ext == "php") || !filename.is_file() {
        notice(options, include);
        return None;
    }
    if filename.ends_with("phpstan-baseline.neon") && options.baseline.is_none() {
        options.baseline = Some(crate::project::display_path(&filename));
        return None;
    }
    Some(Visit::Read(filename))
}

fn composer_package(path: &Path) -> Option<String> {
    let components = path.iter().collect::<Vec<_>>();
    if let Some(vendor) = components.iter().position(|part| *part == "vendor") {
        let owner = components.get(vendor + 1)?.to_str()?;
        let package = components.get(vendor + 2)?.to_str()?;
        return Some(format!("{owner}/{package}"));
    }
    let package = components.iter().find_map(|part| {
        let name = part.to_str()?;
        name.starts_with("phpstan-").then_some(name)
    })?;
    // A relative reference to a package installed next to phpstan/phpstan.
    components
        .iter()
        .any(|part| *part == "%rootDir%")
        .then(|| format!("phpstan/{package}"))
}

fn parameters(
    records: &[&str],
    directory: &Path,
    parent: &Path,
    options: &mut Options,
) -> Result<(), String> {
    for (line, children) in blocks(records) {
        let Some((key, value)) = key_value(line) else {
            continue;
        };
        match key {
            "level" => {
                if let Some(value) = scalar(value) {
                    options.level = level(&value)?;
                }
            }
            "paths" | "paths!" => {
                if key.ends_with('!') {
                    options.paths.clear();
                }
                options.paths.extend(
                    list(value, children)
                        .iter()
                        .map(|path| resolved_text(path, directory, parent)),
                );
            }
            "excludePaths" | "excludePaths!" => {
                if key.ends_with('!') {
                    options.exclude.clear();
                }
                let exclusions = list(value, children).into_iter().chain(
                    blocks(children)
                        .filter_map(|(line, paths)| {
                            let (key, value) = key_value(line)?;
                            matches!(key, "analyse" | "analyseAndScan").then(|| list(value, paths))
                        })
                        .flatten(),
                );
                options
                    .exclude
                    .extend(exclusions.map(|path| resolved_text(&path, directory, parent)));
            }
            "reportAlwaysTrueInLastCondition" => {
                options.report_always_true_last_condition = value == "true";
            }
            "reportUnmatchedIgnoredErrors" => {
                options.report_unmatched_ignores = value == "true";
            }
            "treatPhpDocTypesAsCertain" => {
                options.treat_phpdoc_types_as_certain = value != "false";
            }
            "checkMissingTypehints" => {
                options.check_missing_typehints = value.parse().ok();
            }
            "checkPhpDocMissingReturn" => {
                options.check_phpdoc_missing_return = value.parse().ok();
            }
            "featureToggles" => {
                for (line, _) in blocks(children) {
                    match key_value(line) {
                        Some(("unnecessaryNullCoalesce", value)) => {
                            options.unnecessary_null_coalesce = value == "true";
                        }
                        Some(("internalTag", value)) => options.internal_tag = value == "true",
                        _ => {}
                    }
                }
            }
            "ignoreErrors" | "ignoreErrors!" => {
                if key.ends_with('!') {
                    options.phpstan_ignores.clear();
                }
                for entry in ignores::read(value, children, |path| {
                    resolved_text(path, directory, parent)
                }) {
                    match entry {
                        Ok(ignore) => options.phpstan_ignores.push(ignore),
                        Err(reason) => notice(options, &format!("ignoreErrors ({reason})")),
                    }
                }
            }
            key if key.starts_with("checkMissing") || key.starts_with("report") => {
                notice(options, key);
            }
            _ => {}
        }
    }
    Ok(())
}

fn resolve(path: &str, directory: &Path, parent: &Path) -> PathBuf {
    path.strip_prefix("%currentWorkingDirectory%/")
        .map_or_else(|| parent.join(path), |path| directory.join(path))
}

fn resolved_text(path: &str, directory: &Path, parent: &Path) -> String {
    crate::project::display_path(&resolve(path, directory, parent))
}

fn notice(options: &mut Options, feature: &str) {
    options
        .config_notices
        .insert(format!("PHPStan configuration {feature}: not applied"));
}
