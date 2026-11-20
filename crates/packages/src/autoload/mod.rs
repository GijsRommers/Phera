//! Build standalone Composer autoload data, e.g. `phera dump-autoload -o`.
//!
//! Root mappings override dependencies; dependency files run before root files.

mod discovery;
mod emit;
mod expressions;
mod order;
mod parse;
mod platform;
mod scan;

use crate::{Options, Package, Project, project::strings};
use anyhow::{Result, bail};
use md5::{Digest, Md5};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct Maps {
    psr4: BTreeMap<String, Vec<PathBuf>>,
    psr0: BTreeMap<String, Vec<PathBuf>>,
    classes: BTreeMap<String, PathBuf>,
    files: Vec<(String, PathBuf)>,
}

pub(crate) fn generate(project: &Project, packages: &[Package], options: &Options) -> Result<()> {
    project.validate_output_paths()?;
    let vendor = project.vendor_dir()?;
    std::fs::create_dir_all(vendor.join("composer"))?;
    let mut maps = Maps::default();
    let mut ordered = order::packages(packages);
    let root = Package {
        name: project.name().into(),
        version: project.data["version"].as_str().unwrap_or("1.0.0").into(),
        data: project.data.as_object().cloned().unwrap_or_default(),
    };
    ordered.push(&root);
    let mut scans = Vec::new();
    let mut global_exclusions = Vec::new();
    for package in ordered.iter().rev() {
        if package.kind() == "metapackage" {
            continue;
        }
        let base = if std::ptr::eq(*package, &root) {
            project.directory.clone()
        } else {
            vendor.join(&package.name)
        };
        let mut autoloads = vec![package.field("autoload")];
        if std::ptr::eq(*package, &root) && !options.no_dev {
            autoloads.push(package.field("autoload-dev"));
        }
        let mut exclusions = Vec::new();
        for autoload in &autoloads {
            exclusions.extend(
                strings(&autoload["exclude-from-classmap"])
                    .iter()
                    .map(|text| (*text).to_owned()),
            );
        }
        global_exclusions.extend(scan::patterns(&base, &exclusions)?);
        for autoload in autoloads {
            collect_mappings(autoload, &base, &mut maps)?;
            for path in strings(&autoload["classmap"]) {
                let path = legacy_path(package, &base, path);
                scans.push(scan::Source::new(&base, &path, &exclusions, None)?);
            }
            if options.optimize
                || project.config_bool("optimize-autoloader")
                || project.config_bool("classmap-authoritative")
            {
                for (key, psr4) in [("psr-4", true), ("psr-0", false)] {
                    if let Some(prefixes) = autoload[key].as_object() {
                        for (prefix, paths) in prefixes {
                            for path in strings(paths) {
                                scans.push(scan::Source::new(
                                    &base,
                                    path,
                                    &exclusions,
                                    Some((prefix.clone(), psr4)),
                                )?);
                            }
                        }
                    }
                }
            }
        }
    }
    for source in &mut scans {
        source.exclusions.extend(global_exclusions.iter().cloned());
    }
    let store = crate::store::Store::open(options.cache_dir.as_deref())?;
    maps.classes = scan::classes(&scans, &vendor, &store.directory.join("autoload"))?;
    maps.classes.insert(
        "Composer\\InstalledVersions".into(),
        vendor.join("composer/InstalledVersions.php"),
    );
    for package in ordered {
        if package.kind() == "metapackage" {
            continue;
        }
        let base = if std::ptr::eq(package, &root) {
            &project.directory
        } else {
            &vendor.join(&package.name)
        };
        let mut autoloads = vec![package.field("autoload")];
        if std::ptr::eq(package, &root) && !options.no_dev {
            autoloads.push(package.field("autoload-dev"));
        }
        for autoload in autoloads {
            for path in strings(&autoload["files"]) {
                let identifier = format!(
                    "{:x}",
                    Md5::digest(format!("{}:{path}", package.name).as_bytes())
                );
                let path = legacy_path(package, base, path);
                maps.files
                    .push((identifier, crate::paths::inside(base, &path)?));
            }
        }
    }
    emit::write(project, &maps, options)
}

fn legacy_path(package: &Package, base: &Path, path: &str) -> String {
    if !base.join(path).exists()
        && let Some(target) = package.field("target-dir").as_str()
    {
        return format!("{target}/{path}");
    }
    path.into()
}

fn collect_mappings(autoload: &Value, base: &Path, maps: &mut Maps) -> Result<()> {
    for (key, target) in [("psr-4", &mut maps.psr4), ("psr-0", &mut maps.psr0)] {
        if let Some(prefixes) = autoload[key].as_object() {
            for (prefix, paths) in prefixes {
                if key == "psr-4" && !prefix.is_empty() && !prefix.ends_with('\\') {
                    bail!("PSR-4 prefix {prefix:?} must end with a backslash.");
                }
                let destination = target
                    .entry(prefix.trim_start_matches('\\').into())
                    .or_default();
                for path in strings(paths) {
                    destination.push(crate::paths::inside(base, path)?);
                }
            }
        }
    }
    Ok(())
}
