//! Inspect installed package metadata and explain a proposed locked version change.

use crate::version::{Constraint, Version};
use crate::{
    Lock, Options, Package, Project, installed, project::strings, registry::Registry, solver,
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::fmt::Write;
use std::path::Path;

pub fn show(directory: &Path, name: Option<&str>, options: &Options) -> Result<String> {
    let mut packages = installed::read::packages(&Project::read(directory)?, options.no_dev)?;
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    if let Some(name) = name {
        let package = packages
            .iter()
            .find(|package| package.name.eq_ignore_ascii_case(name))
            .with_context(|| format!("{name} is not installed."))?;
        let mut output = format!("{} {}\n", package.name, package.version);
        for field in ["description", "type", "homepage", "install-path"] {
            if let Some(value) = package.field(field).as_str() {
                writeln!(output, "{field}  {value}")?;
            }
        }
        let license = strings(package.field("license")).join(", ");
        if !license.is_empty() {
            writeln!(output, "license  {license}")?;
        }
        for (name, constraint) in links(package.field("require")) {
            writeln!(output, "requires  {name} {constraint}")?;
        }
        return Ok(output.trim_end().to_owned());
    }
    let mut output = Vec::new();
    for package in packages {
        let description = package.field("description").as_str().unwrap_or("");
        output.push(
            format!("{} {} {description}", package.name, package.version)
                .trim_end()
                .to_owned(),
        );
    }
    Ok(if output.is_empty() {
        "No packages installed.".into()
    } else {
        output.join("\n")
    })
}

pub fn licenses(directory: &Path, options: &Options) -> Result<String> {
    let mut packages = installed::read::packages(&Project::read(directory)?, options.no_dev)?;
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    let mut output = String::from("Package | Version | License\n--- | --- | ---");
    for package in packages {
        let licenses = strings(package.field("license")).join(", ");
        let label = if licenses.is_empty() {
            "Unknown"
        } else {
            &licenses
        };
        write!(output, "\n{} | {} | {label}", package.name, package.version)?;
    }
    Ok(output)
}

pub fn why_not(directory: &Path, name: &str, requested: &str, options: &Options) -> Result<String> {
    let name = &name.to_ascii_lowercase();
    let project = Project::read(directory)?;
    let lock = Lock::read(directory)?;
    let version = Version::parse(requested)?;
    let registry = Registry::new(&project, options)?;
    let candidate = registry
        .packages(name)?
        .into_iter()
        .find(|package| Version::parse(&package.version).is_ok_and(|v| v == version))
        .with_context(|| {
            format!("Cannot find {name} {requested} in the configured repositories.")
        })?;
    let installed = lock.packages(options.no_dev);
    let present: Vec<_> = installed
        .iter()
        .filter(|package| package.name != *name)
        .chain(std::iter::once(&candidate))
        .collect();
    let mut reasons = Vec::new();
    let root = project.root_version();
    for field in ["require", "require-dev", "conflict"] {
        if options.no_dev && field == "require-dev" {
            continue;
        }
        check_candidate(
            &mut reasons,
            project.name(),
            &root,
            field,
            &project.data[field],
            &candidate,
        )?;
    }
    for package in installed.iter().filter(|package| package.name != *name) {
        for field in ["require", "conflict"] {
            let owner = format!("{} {}", package.name, package.version);
            check_candidate(
                &mut reasons,
                &owner,
                &package.version,
                field,
                package.field(field),
                &candidate,
            )?;
        }
    }
    let platform = solver::platform(&project, options)?;
    for field in ["require", "conflict"] {
        for (dependency, wanted) in links(candidate.field(field)) {
            let constraint = Constraint::parse(actual_constraint(wanted, &candidate.version))?;
            let mut compatible = false;
            if solver::is_platform(dependency) {
                if options.ignores_platform(dependency) {
                    continue;
                }
                compatible = platform
                    .get(dependency)
                    .is_some_and(|version| constraint.matches(version));
            }
            for package in &present {
                compatible |= satisfies(package, dependency, &constraint)?;
            }
            if compatible == (field == "conflict") {
                reasons.push(format!(
                    "{} {} {field} {dependency} {wanted}",
                    candidate.name, candidate.version
                ));
            }
        }
    }
    reasons.sort();
    reasons.dedup();
    if reasons.is_empty() {
        Ok(format!(
            "No blockers found for {name} {requested} in the current lock."
        ))
    } else {
        Ok(reasons.join("\n"))
    }
}

fn check_candidate(
    reasons: &mut Vec<String>,
    owner: &str,
    version: &str,
    field: &str,
    data: &Value,
    candidate: &Package,
) -> Result<()> {
    for (dependency, wanted) in links(data) {
        if dependency != candidate.name
            && candidate.field("provide").get(dependency).is_none()
            && candidate.field("replace").get(dependency).is_none()
        {
            continue;
        }
        let constraint = Constraint::parse(actual_constraint(wanted, version))?;
        if satisfies(candidate, dependency, &constraint)? == (field == "conflict") {
            reasons.push(format!("{owner} {field} {dependency} {wanted}"));
        }
    }
    Ok(())
}

pub(crate) fn links(value: &Value) -> impl Iterator<Item = (&str, &str)> {
    value
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(name, value)| Some((name.as_str(), value.as_str()?)))
}

pub(crate) fn actual_constraint<'a>(text: &'a str, version: &'a str) -> &'a str {
    if text == "self.version" {
        version
    } else {
        text
    }
}

pub(crate) fn satisfies(package: &Package, name: &str, wanted: &Constraint) -> Result<bool> {
    if package.name == name {
        if wanted.matches(&Version::parse(&package.version)?) {
            return Ok(true);
        }
        if let Some(alias) = package.field("extra")["branch-alias"][&package.version].as_str()
            && Version::branch_alias(alias).is_ok_and(|version| wanted.matches(&version))
        {
            return Ok(true);
        }
    }
    for field in ["provide", "replace"] {
        if let Some(offer) = package.field(field)[name].as_str()
            && wanted.intersects(&Constraint::parse(actual_constraint(
                offer,
                &package.version,
            ))?)
        {
            return Ok(true);
        }
    }
    Ok(false)
}
