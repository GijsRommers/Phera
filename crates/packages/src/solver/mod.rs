//! PubGrub dependency selection with Composer candidate preferences.
//!
//! Root `"require": {"acme/a":"^2"}` pins the solve while PubGrub backtracks transitive choices.

mod catalog;
mod provider;

use crate::{Options, Package, Project, registry::Registry, version::Version};
use anyhow::{Result, bail};
use pubgrub::{DefaultStringReporter, PubGrubError, Reporter, resolve};
use std::collections::BTreeMap;

pub(crate) fn solve(
    project: &Project,
    options: &Options,
    registry: &Registry,
) -> Result<Vec<Package>> {
    let platform = platform(project, options)?;
    let provider = provider::Provider::new(project, options, registry, platform)?;
    let selected = match resolve(&provider, "@root".to_owned(), 1u32) {
        Ok(selected) => selected,
        Err(PubGrubError::NoSolution(tree)) => {
            bail!(
                "Your requirements cannot be installed together.\n{}\n\
                Adjust composer.json requirements or config.platform and run `phera update` again.",
                DefaultStringReporter::report(&tree)
            );
        }
        Err(PubGrubError::ErrorRetrievingDependencies {
            package, source, ..
        }) => {
            return Err(source
                .0
                .context(format!("Cannot resolve requirements of {package}.")));
        }
        Err(PubGrubError::ErrorChoosingVersion { package, source }) => {
            return Err(source
                .0
                .context(format!("Cannot choose a version of {package}.")));
        }
        Err(PubGrubError::ErrorInShouldCancel(source)) => return Err(source.0),
    };
    let mut packages = BTreeMap::new();
    for (name, id) in selected {
        if id == 0 || name == "@root" {
            continue;
        }
        if let Some(package) = provider.package(&name, id)? {
            packages.insert(package.name.clone(), package);
        }
    }
    Ok(packages.into_values().collect())
}

pub(crate) fn is_platform(name: &str) -> bool {
    matches!(
        name,
        "php"
            | "php-64bit"
            | "php-ipv6"
            | "php-zts"
            | "php-debug"
            | "composer-runtime-api"
            | "composer-plugin-api"
            | "composer"
    ) || name.starts_with("ext-")
        || name.starts_with("lib-")
}

fn platform(project: &Project, options: &Options) -> Result<BTreeMap<String, Version>> {
    let mut platform = options
        .platform
        .iter()
        .map(|(name, version)| Ok((name.clone(), Version::parse(version)?)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    platform.insert("composer-runtime-api".into(), Version::parse("2.2.2")?);
    platform.insert("composer-plugin-api".into(), Version::parse("2.6.0")?);
    if let Some(configured) = project.data["config"]["platform"].as_object() {
        for (name, version) in configured {
            if version == false {
                platform.remove(name);
            } else if let Some(version) = version.as_str() {
                platform.insert(name.clone(), Version::parse(version)?);
            }
        }
    }
    Ok(platform)
}
