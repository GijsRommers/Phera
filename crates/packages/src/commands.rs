//! Manifest editing and dependency inspection commands.
//!
//! `phera add psr/log:^3`, `phera remove psr/log`, `phera why psr/log`, `phera outdated`.

use crate::{Lock, Options, Project, registry::Registry, update, version::Version};
use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use serde_json::{Map, Value};
use std::fs;
use std::path::Path;

pub fn add(directory: &Path, spec: &str, development: bool, options: &Options) -> Result<()> {
    add_many(directory, &[spec.to_owned()], development, options)
}

pub fn add_many(
    directory: &Path,
    specs: &[String],
    development: bool,
    options: &Options,
) -> Result<()> {
    if specs.is_empty() {
        bail!("Pass at least one package to add.");
    }
    let mut project = Project::read(directory)?;
    let original = fs::read(project.directory.join("composer.json"))?;
    let key = if development {
        "require-dev"
    } else {
        "require"
    };
    if project.data.get(key).is_none() {
        project.data[key] = Value::Object(Map::new());
    }
    let sort = project.config_bool("sort-packages");
    let requirements = project.data[key]
        .as_object_mut()
        .with_context(|| format!("{key} must be a JSON object."))?;
    let mut names = Vec::new();
    let mut inferred = Vec::new();
    for spec in specs {
        let (name, constraint) = spec.split_once(':').unwrap_or((spec, "*"));
        let name = name.to_ascii_lowercase();
        validate_name(&name)?;
        if names.contains(&name) {
            bail!("{name} was given more than once.");
        }
        crate::version::Constraint::parse(constraint)?;
        requirements.insert(name.clone(), constraint.into());
        if !spec.contains(':') {
            inferred.push((key.to_owned(), name.clone()));
        }
        names.push(name);
    }
    if sort {
        requirements.sort_keys();
    }
    update::run(
        project,
        &names,
        Some(update::Edit { original, inferred }),
        options,
    )
}

pub fn remove(directory: &Path, name: &str, options: &Options) -> Result<()> {
    remove_many(directory, &[name.to_owned()], options)
}

pub fn remove_many(directory: &Path, names: &[String], options: &Options) -> Result<()> {
    if names.is_empty() {
        bail!("Pass at least one package to remove.");
    }
    let mut project = Project::read(directory)?;
    let original = fs::read(project.directory.join("composer.json"))?;
    let names: Vec<_> = names.iter().map(|name| name.to_ascii_lowercase()).collect();
    for name in &names {
        validate_name(name)?;
        let mut found = false;
        for key in ["require", "require-dev"] {
            if let Some(requirements) = project.data[key].as_object_mut() {
                found |= requirements.shift_remove(name).is_some();
            }
        }
        if !found {
            bail!("{name} is not a root dependency in composer.json.");
        }
    }
    update::run(
        project,
        &names,
        Some(update::Edit {
            original,
            inferred: Vec::new(),
        }),
        options,
    )
}

pub fn why(directory: &Path, name: &str) -> Result<String> {
    let project = Project::read(directory)?;
    let lock = Lock::read(directory)?;
    let mut lines = Vec::new();
    for key in ["require", "require-dev"] {
        if let Some(constraint) = project.data[key][name].as_str() {
            lines.push(format!("{} {key} {name} {constraint}", project.name()));
        }
    }
    for package in lock.packages(false) {
        if let Some(constraint) = package.field("require")[name].as_str() {
            lines.push(format!(
                "{} {} requires {name} {constraint}",
                package.name, package.version
            ));
        }
    }
    if lines.is_empty() {
        bail!("No installed package requires {name}.");
    }
    Ok(lines.join("\n"))
}

pub fn outdated(directory: &Path, options: &Options) -> Result<String> {
    let project = Project::read(directory)?;
    let lock = Lock::read(directory)?;
    let registry = Registry::new(&project, options)?;
    let packages = lock.packages(options.no_dev);
    let mut lines: Vec<String> = packages
        .par_iter()
        .map(|package| -> Result<Option<String>> {
            let current = Version::parse(&package.version)?;
            let latest = registry
                .packages(&package.name)?
                .into_iter()
                .filter_map(|package| {
                    Version::parse(&package.version)
                        .ok()
                        .map(|version| (version, package))
                })
                .filter(|(version, _)| version.stability() == crate::version::Stability::Stable)
                .max_by(|(left, _), (right, _)| left.cmp(right));
            Ok(latest
                .filter(|(version, _)| *version > current)
                .map(|(_, latest)| {
                    format!("{} {} -> {}", package.name, package.version, latest.version)
                }))
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    lines.sort();
    Ok(if lines.is_empty() {
        "All packages are up to date.".into()
    } else {
        lines.join("\n")
    })
}

fn validate_name(name: &str) -> Result<()> {
    crate::Package {
        name: name.into(),
        version: "1.0.0".into(),
        data: Map::new(),
    }
    .validate()
}
