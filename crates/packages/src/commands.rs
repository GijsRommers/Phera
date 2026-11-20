//! Manifest editing and dependency inspection commands.
//!
//! `phera add psr/log:^3`, `phera remove psr/log`, `phera why psr/log`, `phera outdated`.

use crate::{Lock, Options, Project, registry::Registry, version::Version};
use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use serde_json::{Map, Value};
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn add(directory: &Path, spec: &str, development: bool, options: &Options) -> Result<()> {
    let mut project = Project::read(directory)?;
    let (name, constraint) = spec
        .split_once(':')
        .map_or((spec, None), |(name, constraint)| (name, Some(constraint)));
    validate_name(name)?;
    let constraint = if let Some(constraint) = constraint {
        crate::version::Constraint::parse(constraint)?;
        constraint.to_owned()
    } else {
        let registry = Registry::new(&project, options)?;
        let latest = registry
            .packages(name)?
            .into_iter()
            .filter_map(|package| {
                Version::parse(&package.version)
                    .ok()
                    .map(|version| (version, package))
            })
            .filter(|(version, _)| version.stability() == crate::version::Stability::Stable)
            .max_by(|(left, _), (right, _)| left.cmp(right))
            .context("No stable version found; pass an explicit vendor/package:constraint.")?;
        format!("^{}", latest.1.version.trim_start_matches('v'))
    };
    let key = if development {
        "require-dev"
    } else {
        "require"
    };
    if project.data.get(key).is_none() {
        project.data[key] = Value::Object(Map::new());
    }
    project.data[key]
        .as_object_mut()
        .context("require must be a JSON object.")?
        .insert(name.into(), constraint.into());
    if project.config_bool("sort-packages") {
        let requirements = project.data[key]
            .as_object_mut()
            .context("Invalid require object.")?;
        requirements.sort_keys();
    }
    edit_and_update(&project, options)
}

pub fn remove(directory: &Path, name: &str, options: &Options) -> Result<()> {
    let mut project = Project::read(directory)?;
    validate_name(name)?;
    let mut found = false;
    for key in ["require", "require-dev"] {
        if let Some(requirements) = project.data[key].as_object_mut() {
            found |= requirements.remove(name).is_some();
        }
    }
    if !found {
        bail!("{name} is not a root dependency in composer.json.");
    }
    edit_and_update(&project, options)
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

fn edit_and_update(project: &Project, options: &Options) -> Result<()> {
    let manifest_path = project.directory.join("composer.json");
    let original = fs::read(&manifest_path)?;
    let lock_path = project.directory.join("composer.lock");
    let original_lock = fs::read(&lock_path).ok();
    let mut file = tempfile::NamedTempFile::new_in(&project.directory)?;
    file.write_all((serde_json::to_string_pretty(&project.data)? + "\n").as_bytes())?;
    file.persist(&manifest_path)?;
    if let Err(error) = crate::update(&project.directory, options) {
        fs::write(manifest_path, original)?;
        match original_lock {
            Some(bytes) => fs::write(lock_path, bytes)?,
            None if lock_path.exists() => fs::remove_file(lock_path)?,
            None => {}
        }
        return Err(error.context("Restored composer.json and composer.lock after failed update."));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<()> {
    crate::Package {
        name: name.into(),
        version: "1.0.0".into(),
        data: Map::new(),
    }
    .validate()
}
