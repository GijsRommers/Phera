//! Live Composer path links, private mirrors and cached offline snapshots.
//!
//! `{"type":"path","url":"packages/*","options":{"symlink":false}}`
//! explicitly requests a mirror instead of a link to project-local sources.

use crate::{Package, Project};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn destination(vendor: &Path, package: &Package) -> Result<PathBuf> {
    package.validate()?;
    let (owner, name) = package
        .name
        .split_once('/')
        .context("Invalid package name.")?;
    Ok(crate::paths::inside(vendor, owner)?.join(name))
}

pub(crate) fn link(project: &Project, package: &Package, destination: &Path) -> Result<bool> {
    if package.field("dist")["type"] != "path"
        || package.field("transport-options")["symlink"] == false
    {
        return Ok(false);
    }
    let url = package.field("dist")["url"]
        .as_str()
        .context("Path distribution needs its url.")?;
    let source = crate::paths::inside(&project.directory, url)?;
    if !source.exists() {
        return Ok(false);
    }
    let source = source.canonicalize()?;
    if !source.starts_with(&project.directory) {
        bail!("Path distributions must stay inside the project directory.");
    }
    #[cfg(unix)]
    {
        let parent = destination
            .parent()
            .context("Path package has no parent directory.")?;
        fs::create_dir_all(parent)?;
        let staging = tempfile::tempdir_in(parent)?;
        let link = staging.path().join("package");
        let mut target = if package.field("transport-options")["relative"] == false {
            source
        } else {
            PathBuf::from(crate::php::relative(parent, &source))
        };
        // Composer preserves a trailing separator in directory link targets.
        target.push("");
        std::os::unix::fs::symlink(target, &link)?;
        remove(destination)?;
        fs::rename(link, destination)?;
        Ok(true)
    }
    #[cfg(not(unix))]
    {
        let _ = destination;
        Ok(false)
    }
}

pub(crate) fn remove(destination: &Path) -> Result<()> {
    if let Ok(metadata) = destination.symlink_metadata() {
        if metadata.is_symlink() || metadata.is_file() {
            fs::remove_file(destination)?;
        } else {
            fs::remove_dir_all(destination)?;
        }
    }
    Ok(())
}
