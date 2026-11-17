//! Validate locked platform requirements before downloading or changing vendor.
//!
//! A lock requiring `"php": "^8.5"` rejects PHP 8.4 unless config.platform overrides it.

use crate::{Lock, Options, Package, Project, solver, version::Constraint};
use anyhow::{Context, Result, bail};
use serde_json::Value;

pub(crate) fn validate(project: &Project, lock: &Lock, options: &Options) -> Result<()> {
    if options.ignore_platform {
        return Ok(());
    }
    let platform = solver::platform(project, options)?;
    let packages = lock.packages(options.no_dev);
    let root = Package {
        name: project.name().into(),
        version: project.root_version(),
        data: project.data.as_object().cloned().unwrap_or_default(),
    };
    let mut requirements = vec![(root.name.as_str(), root.field("require"), false)];
    if !options.no_dev {
        requirements.push((root.name.as_str(), root.field("require-dev"), false));
    }
    requirements.push((root.name.as_str(), root.field("conflict"), true));
    for package in &packages {
        requirements.push((&package.name, package.field("require"), false));
        requirements.push((&package.name, package.field("conflict"), true));
    }
    for (owner, links, negative) in requirements {
        let Some(links) = links.as_object() else {
            continue;
        };
        for (name, text) in links.iter().filter(|(name, _)| solver::is_platform(name)) {
            let text = text
                .as_str()
                .context("Platform constraint must be a string.")?;
            let constraint = Constraint::parse(text)?;
            let mut matched = platform
                .get(name)
                .is_some_and(|version| constraint.matches(version));
            for provider in packages.iter().chain(std::iter::once(&root)) {
                matched |= offers(provider, name, &constraint)?;
            }
            if matched == negative {
                let relation = if negative {
                    "conflicts with"
                } else {
                    "requires"
                };
                bail!(
                    "{owner} {relation} {name} {text}. The locked packages do not match \
                    this Phera platform. Use a compatible runtime or correct config.platform."
                );
            }
        }
    }
    Ok(())
}

fn offers(package: &Package, name: &str, wanted: &Constraint) -> Result<bool> {
    for key in ["provide", "replace"] {
        if let Some(text) = package.field(key).get(name).and_then(Value::as_str) {
            let text = if text == "self.version" {
                &package.version
            } else {
                text
            };
            if wanted.intersects(&Constraint::parse(text)?) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
