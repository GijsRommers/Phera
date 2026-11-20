//! Read Composer's installed package records from the configured vendor directory.

use crate::{Package, Project, paths, project::strings};
use anyhow::{Context, Result};
use serde_json::Value;
use std::fs;

pub(crate) fn packages(project: &Project, no_dev: bool) -> Result<Vec<Package>> {
    let file = paths::inside(&project.vendor_dir()?, "composer/installed.json")?;
    let bytes = fs::read(file).context("No installed packages. Run `phera install` first.")?;
    let metadata: Value = serde_json::from_slice(&bytes).context("Invalid installed.json.")?;
    let dev = strings(&metadata["dev-package-names"]);
    let packages = metadata["packages"]
        .as_array()
        .context("Missing installed package records.")?;
    packages
        .iter()
        .filter(|package| {
            !no_dev
                || !package["name"]
                    .as_str()
                    .is_some_and(|name| dev.contains(&name))
        })
        .map(|package| Ok(serde_json::from_value(package.clone())?))
        .collect()
}
