//! A single directory plan for unpacking and Composer's installed metadata.
//!
//! An approved installer can map `drupal/core` to `web/core` before any files are copied.

use crate::{Options, Package, Project, path_install, paths, php, plugins};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

pub(crate) struct InstallPaths {
    destinations: BTreeMap<String, PathBuf>,
}

impl InstallPaths {
    pub(crate) fn new(project: &Project, options: &Options, packages: &[Package]) -> Result<Self> {
        let vendor = project.vendor_dir()?;
        let bin = project.bin_dir()?;
        let installers = plugins::active(project, options, packages, plugins::layout::INSTALLERS)?;
        let wordpress =
            plugins::active(project, options, packages, plugins::layout::WORDPRESS_CORE)?;
        let mut destinations = BTreeMap::new();
        for package in packages {
            package.validate()?;
            if package.kind() == "metapackage" {
                continue;
            }
            let destination = match plugins::layout::custom(project, package, installers, wordpress)
            {
                Some(path) => {
                    let relative = paths::relative(&path)?;
                    if relative.components().any(|part| match part {
                        Component::Normal(name) => name.to_string_lossy().starts_with('.'),
                        _ => false,
                    }) {
                        bail!("Custom package paths cannot use hidden directories.");
                    }
                    let destination = paths::inside(&project.directory, &path)?;
                    if overlaps(&destination, &vendor) || overlaps(&destination, &bin) {
                        bail!("Custom path for {} overlaps vendor or bin.", package.name);
                    }
                    destination
                }
                None => path_install::destination(&vendor, package)?,
            };
            if destinations
                .insert(package.name.clone(), destination)
                .is_some()
            {
                bail!("composer.lock contains duplicate package {}.", package.name);
            }
        }
        let mut directories: Vec<_> = destinations
            .values()
            .map(|path| PathBuf::from(path.to_string_lossy().to_ascii_lowercase()))
            .collect();
        directories.sort();
        if directories
            .windows(2)
            .any(|pair| overlaps(&pair[0], &pair[1]))
        {
            bail!("Two packages have overlapping installation directories.");
        }
        Ok(Self { destinations })
    }

    pub(crate) fn destination(&self, package: &Package) -> Result<&Path> {
        self.destinations
            .get(&package.name)
            .map(PathBuf::as_path)
            .with_context(|| format!("No installation directory for {}.", package.name))
    }

    pub(crate) fn installed(&self, package: &Package) -> Result<PathBuf> {
        let base = self.destination(package)?;
        paths::inside(base, package.field("target-dir").as_str().unwrap_or(""))
    }

    pub(crate) fn validate_replacement(
        &self,
        project: &Project,
        packages: &[Package],
        previous: &[Package],
    ) -> Result<()> {
        let vendor = project.vendor_dir()?;
        let composer = vendor.join("composer");
        for package in packages
            .iter()
            .filter(|package| package.kind() != "metapackage")
        {
            let destination = self.destination(package)?;
            if destination.starts_with(&vendor) || destination.symlink_metadata().is_err() {
                continue;
            }
            let expected = php::relative(&composer, &self.installed(package)?);
            let owned = previous.iter().any(|old| {
                old.name == package.name
                    && old.field("install-path").as_str() == Some(expected.as_str())
            });
            if !owned {
                bail!(
                    "Refusing to replace {} at an existing custom path \
                     without an installed record: {}.",
                    package.name,
                    destination.display()
                );
            }
        }
        Ok(())
    }
}

fn overlaps(first: &Path, second: &Path) -> bool {
    first.starts_with(second) || second.starts_with(first)
}
