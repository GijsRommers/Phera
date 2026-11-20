//! Populate Drupal's `[web-root]/index.php` and its vendor loader references.
//!
//! The mapping in `extra.drupal-scaffold` describes files to copy from installed packages.

use crate::{InstallPaths, Package, Project, paths, php, project::strings};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

pub(super) const NAME: &str = "drupal/core-composer-scaffold";

pub(super) fn write(
    project: &Project,
    packages: &[Package],
    installed: &InstallPaths,
) -> Result<()> {
    let config = &project.data["extra"]["drupal-scaffold"];
    let allowed = strings(&config["allowed-packages"]);
    let vendor = project.vendor_dir()?;
    let mut files = BTreeMap::new();
    for package in packages {
        if package.name == "drupal/core" || allowed.contains(&package.name.as_str()) {
            let map = &package.field("extra")["drupal-scaffold"]["file-mapping"];
            if map.is_object() {
                let source = installed.installed(package)?;
                mappings(project, config, &vendor, &source, map, &mut files)?;
            }
        }
    }
    mappings(
        project,
        config,
        &vendor,
        &project.directory,
        &config["file-mapping"],
        &mut files,
    )?;
    let web = location(project, config, "web-root")?;
    for filename in ["autoload.php", "autoload_runtime.php"] {
        let destination = output_path(project, &vendor, &web.join(filename))?;
        let reference = php::relative(&web, &vendor.join(filename));
        let runtime = if filename == "autoload_runtime.php" {
            concat!(
                "$_ENV['APP_RUNTIME'] ??= $_SERVER['APP_RUNTIME'] ?? ",
                "\\Drupal\\Core\\Runtime\\DrupalRuntime::class;\n",
            )
        } else {
            ""
        };
        let code = format!(
            "<?php\n\n{runtime}return require __DIR__ . {};\n",
            php::quote(&format!("/{reference}")),
        );
        files.insert(destination, code.into_bytes());
    }
    for (path, contents) in files {
        create(&path, &contents)?;
    }
    Ok(())
}

fn mappings(
    project: &Project,
    config: &Value,
    vendor: &Path,
    source: &Path,
    map: &Value,
    files: &mut BTreeMap<PathBuf, Vec<u8>>,
) -> Result<()> {
    let Some(map) = map.as_object() else {
        return Ok(());
    };
    for (destination, operation) in map {
        let (location_name, name) = destination
            .strip_prefix('[')
            .and_then(|target| target.split_once("]/"))
            .context("Drupal scaffold targets must start with a location such as [web-root]/.")?;
        let base = location(project, config, location_name)?;
        let target = output_path(project, vendor, &paths::inside(&base, name)?)?;
        if operation == false {
            files.remove(&target);
            continue;
        }
        let input = operation
            .as_str()
            .or_else(|| operation["path"].as_str())
            .with_context(|| format!("Unsupported Drupal scaffold mapping for {destination}."))?;
        let input = paths::inside(source, input)?;
        files.insert(target, fs::read(input)?);
    }
    Ok(())
}

fn location(project: &Project, config: &Value, name: &str) -> Result<PathBuf> {
    let relative = config["locations"][name]
        .as_str()
        .or_else(|| matches!(name, "project-root" | "web-root").then_some("."))
        .with_context(|| format!("Unknown Drupal scaffold location {name}."))?;
    paths::inside(&project.directory, relative)
}

fn output_path(project: &Project, vendor: &Path, destination: &Path) -> Result<PathBuf> {
    let relative = destination.strip_prefix(&project.directory)?;
    let protected = relative.components().any(|part| {
        matches!(part, Component::Normal(name)
            if name.to_string_lossy().eq_ignore_ascii_case(".git"))
    }) || matches!(
        relative.to_str(),
        Some("composer.json" | "composer.lock" | "auth.json")
    );
    if protected || relative.as_os_str().is_empty() || destination.starts_with(vendor) {
        bail!(
            "Protected Drupal scaffold destination: {}.",
            destination.display()
        );
    }
    paths::inside(&project.directory, &relative.to_string_lossy())
}

fn create(path: &Path, contents: &[u8]) -> Result<()> {
    if path.symlink_metadata().is_ok() {
        return Ok(());
    }
    let parent = path
        .parent()
        .context("Drupal scaffold file needs a parent directory.")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(contents)?;
    file.persist_noclobber(path)?;
    crate::archive::set_mode(path, 0o644)?;
    Ok(())
}
