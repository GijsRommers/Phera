//! Unpack a project's root template using the package registry and content store.

use crate::{
    Options, Project,
    network::Network,
    registry::Registry,
    store,
    version::{Constraint, Stability, Version},
};
use anyhow::{Context, Result, bail};
use serde_json::json;
use std::fs;
use std::path::Path;

pub fn fetch_project(directory: &Path, name: &str, wanted: &str, options: &Options) -> Result<()> {
    let directory = directory.canonicalize()?;
    if fs::read_dir(&directory)?.next().is_some() {
        bail!("The template destination must be empty.");
    }
    let project = Project {
        directory,
        data: json!({}),
    };
    let constraint = Constraint::parse(wanted)?;
    let registry = Registry::new(&project, options)?;
    let package = registry
        .packages(name)?
        .into_iter()
        .filter_map(|package| {
            let version = Version::parse(&package.version).ok()?;
            (version.stability() == Stability::Stable && constraint.matches(&version))
                .then_some((version, package))
        })
        .max_by(|(left, _), (right, _)| left.cmp(right))
        .map(|(_, package)| package)
        .with_context(|| format!("No stable template {name} matches {wanted}."))?;
    let cache = store::Store::open(options.cache_dir.as_deref())?;
    let network = Network::new(&project)?;
    let files = cache.prepare(&package, &network, options.offline, &project.directory)?;
    store::copy_tree(&files, &project.directory)?;
    Ok(())
}
