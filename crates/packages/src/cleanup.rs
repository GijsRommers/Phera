//! Remove packages and proxies owned by the previous installed.json.
//!
//! Switching to `phera install --no-dev` removes previously installed dev packages.

use crate::{Package, Project, project::strings};
use anyhow::Result;
use serde_json::Value;
use std::fs;

pub(crate) fn previous(project: &Project) -> Result<Vec<Package>> {
    let file = project.vendor_dir()?.join("composer/installed.json");
    if !file.is_file() {
        return Ok(Vec::new());
    }
    let record: Value = serde_json::from_slice(&fs::read(file)?)?;
    Ok(serde_json::from_value(record["packages"].clone())?)
}

pub(crate) fn remove_stale(
    project: &Project,
    previous: &[Package],
    current: &[Package],
) -> Result<()> {
    let vendor = project.vendor_dir()?;
    let bin = project.bin_dir()?;
    for package in previous {
        package.validate()?;
        if !current.iter().any(|new| new.name == package.name) {
            let destination = crate::paths::inside(&vendor, &package.name)?;
            if destination.is_dir() {
                fs::remove_dir_all(destination)?;
            }
        }
        for path in strings(package.field("bin")) {
            let path = crate::paths::relative(path)?;
            if let Some(name) = path.file_name() {
                let proxy = crate::paths::inside(&bin, &name.to_string_lossy())?;
                if proxy.is_file() {
                    fs::remove_file(proxy)?;
                }
            }
        }
    }
    Ok(())
}
