//! Validate composer.json and its lock without downloading or installing packages.

use crate::{
    Lock, Package, Project, content_hash, inspect, solver,
    version::{Constraint, Version},
};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

pub fn validate(directory: &Path) -> Result<String> {
    let project = Project::read(directory)?;
    validate_links(
        &project.data,
        &["require", "require-dev", "provide", "replace", "conflict"],
    )?;
    project.validate_output_paths()?;
    if !directory.join("composer.lock").try_exists()? {
        return Ok("composer.json is valid.".into());
    }
    let lock = Lock::read(directory)?;
    if lock.content_hash != content_hash::compute(&project.data)? {
        bail!("composer.lock is out of date. Run `phera update`.");
    }
    let packages = lock.packages(false);
    let mut seen = BTreeSet::new();
    for package in &packages {
        package.validate()?;
        Version::parse(&package.version)?;
        if !seen.insert(package.name.as_str()) {
            bail!("composer.lock contains duplicate package {}.", package.name);
        }
        validate_links(
            &Value::Object(package.data.clone()),
            &["require", "provide", "replace", "conflict"],
        )?;
    }
    let root = Package {
        name: project.name().to_owned(),
        version: project.root_version(),
        data: project.data.as_object().cloned().unwrap_or_default(),
    };
    for owner in packages.iter().chain(std::iter::once(&root)) {
        for section in ["require", "require-dev"] {
            if section == "require-dev" && !std::ptr::eq(owner, &root) {
                continue;
            }
            for (name, wanted) in inspect::links(owner.field(section)) {
                if solver::is_platform(name) {
                    continue;
                }
                let constraint =
                    Constraint::parse(inspect::actual_constraint(wanted, &owner.version))?;
                let mut found = false;
                for candidate in packages.iter().chain(std::iter::once(&root)) {
                    found |= inspect::satisfies(candidate, name, &constraint)?;
                }
                if !found {
                    found = lock
                        .other
                        .get("aliases")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .any(|alias| {
                            alias["package"] == name
                                && alias["alias_normalized"]
                                    .as_str()
                                    .and_then(|version| Version::parse(version).ok())
                                    .is_some_and(|version| constraint.matches(&version))
                        });
                }
                if !found {
                    bail!(
                        "{} needs {name} {wanted}, which composer.lock does not satisfy.",
                        owner.name
                    );
                }
            }
        }
    }
    Ok("composer.json and composer.lock are valid and consistent.".into())
}

fn validate_links(data: &Value, sections: &[&str]) -> Result<()> {
    for section in sections {
        let value = &data[*section];
        if value.is_null() || value.as_array().is_some_and(Vec::is_empty) {
            continue;
        }
        let requirements = value
            .as_object()
            .with_context(|| format!("{section} must be an object."))?;
        for (name, constraint) in requirements {
            if !solver::is_platform(name) {
                Package {
                    name: name.clone(),
                    version: "1.0.0".into(),
                    data: Default::default(),
                }
                .validate()?;
            }
            let text = constraint
                .as_str()
                .with_context(|| format!("{section}.{name} needs a constraint string."))?;
            if text != "self.version" {
                Constraint::parse(text)
                    .with_context(|| format!("Invalid constraint for {name}."))?;
            }
        }
    }
    Ok(())
}
