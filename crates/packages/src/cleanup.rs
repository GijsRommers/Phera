//! Remove packages and proxies owned by the previous installed.json.
//!
//! Switching to `phera install --no-dev` removes previously installed dev packages.

use crate::{InstallPaths, Package, Project, php, project::strings};
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
    current: &InstallPaths,
) -> Result<()> {
    let vendor = project.vendor_dir()?;
    let bin = project.bin_dir()?;
    for package in previous {
        package.validate()?;
        let vendor_path = crate::path_install::destination(&vendor, package)?;
        let installed = package.install_path(&vendor)?;
        let expected = php::relative(&vendor.join("composer"), &installed);
        if package
            .field("install-path")
            .as_str()
            .is_some_and(|path| path.strip_prefix("./").unwrap_or(path) == expected)
            && current.destination(package).ok() != Some(vendor_path.as_path())
        {
            crate::path_install::remove(&vendor_path)?;
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
