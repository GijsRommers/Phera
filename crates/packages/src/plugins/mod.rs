//! Native install hooks enabled by `"config": {"allow-plugins": {"symfony/runtime": true}}`.
//!
//! Hooks consume package data after resolution. They never load Composer plugin PHP.

mod drupal;
pub(crate) mod layout;
mod runtime;

use crate::{InstallPaths, Options, Package, Project};
use anyhow::{Context, Result, bail};
use serde_json::Value;

const NATIVE: &[&str] = &[
    runtime::NAME,
    layout::INSTALLERS,
    layout::WORDPRESS_CORE,
    drupal::NAME,
];

pub(crate) fn allowed(project: &Project, name: &str) -> Result<Option<bool>> {
    let rules = &project.data["config"]["allow-plugins"];
    if let Some(allow) = rules.as_bool() {
        return Ok(Some(allow));
    }
    if let Some(rules) = rules.as_object() {
        for (pattern, allow) in rules {
            let pattern = regex::escape(pattern).replace("\\*", ".*");
            if regex::Regex::new(&format!("(?i)^{pattern}$"))?.is_match(name) {
                return Ok(Some(allow.as_bool().context(
                    "Each config.allow-plugins entry must be true or false.",
                )?));
            }
        }
    }
    Ok(None)
}

pub(crate) fn active(
    project: &Project,
    options: &Options,
    packages: &[Package],
    name: &str,
) -> Result<bool> {
    if options.no_plugins {
        return Ok(false);
    }
    let Some(package) = packages.iter().find(|package| {
        package.name == name && matches!(package.kind(), "composer-plugin" | "composer-installer")
    }) else {
        return Ok(false);
    };
    match allowed(project, name)? {
        Some(allow) => Ok(allow),
        None if package.field("extra")["plugin-optional"] == Value::Bool(true) => Ok(false),
        None => bail!(
            "{name} is blocked by config.allow-plugins. Set it to true to enable its native hook, \
             or false to explicitly skip it."
        ),
    }
}

pub(crate) fn validate(project: &Project, options: &Options, packages: &[Package]) -> Result<()> {
    for name in NATIVE {
        active(project, options, packages, name)?;
    }
    Ok(())
}

pub(crate) fn post_autoload(
    project: &Project,
    options: &Options,
    packages: &[Package],
) -> Result<()> {
    if active(project, options, packages, runtime::NAME)? {
        runtime::write(project)?;
    }
    Ok(())
}

pub(crate) fn post_install(
    project: &Project,
    options: &Options,
    packages: &[Package],
    paths: &InstallPaths,
) -> Result<()> {
    if active(project, options, packages, drupal::NAME)? {
        drupal::write(project, packages, paths)?;
    }
    Ok(())
}

pub(crate) fn report(project: &Project, options: &Options, packages: &[Package]) -> Result<()> {
    for package in packages {
        if !matches!(package.kind(), "composer-plugin" | "composer-installer") {
            continue;
        }
        if options.no_plugins || allowed(project, &package.name)? == Some(false) {
            continue;
        }
        if !NATIVE.contains(&package.name.as_str()) {
            eprintln!(
                "Skipped Composer plugin {}. Phera cannot apply its install changes yet.",
                package.name
            );
        }
    }
    Ok(())
}
