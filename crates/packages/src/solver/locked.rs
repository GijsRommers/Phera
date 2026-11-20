//! Keep unrelated packages fixed during `phera update vendor/package`.

use crate::{
    Lock, Package, Project,
    registry::Registry,
    version::{Constraint, Version},
};
use anyhow::{Result, bail};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub(crate) struct Locked {
    packages: BTreeMap<String, Package>,
}

impl Locked {
    pub(crate) fn select(
        previous: Option<&Lock>,
        requested: &[String],
        transitive: Option<(&Project, &Registry)>,
    ) -> Result<Self> {
        if requested.is_empty() {
            return Ok(Self::default());
        }
        let mut packages: BTreeMap<_, _> = previous
            .into_iter()
            .flat_map(|lock| lock.packages(false))
            .map(|package| (package.name.clone(), package))
            .collect();
        let mut pending: Vec<(String, String)> = requested
            .iter()
            .map(|name| (name.clone(), "*".into()))
            .collect();
        let mut seen = BTreeSet::new();
        while !packages.is_empty()
            && let Some((name, wanted)) = pending.pop()
        {
            if super::is_platform(&name) || !seen.insert((name.clone(), wanted.clone())) {
                continue;
            }
            packages.remove(&name);
            let Some((project, registry)) = transitive else {
                continue;
            };
            pending.extend(
                packages
                    .values()
                    .filter(|package| offers(package, &name))
                    .map(|package| (package.name.clone(), "*".into())),
            );
            let wanted = Constraint::parse(&wanted)?;
            let root = project.data["require-dev"][&name]
                .as_str()
                .or_else(|| project.data["require"][&name].as_str())
                .map(Constraint::parse)
                .transpose()?;
            for package in registry.packages(&name)? {
                if !matches_candidate(&package, &wanted)
                    || root
                        .as_ref()
                        .is_some_and(|root| !matches_candidate(&package, root))
                {
                    continue;
                }
                for (name, wanted) in package.field("require").as_object().into_iter().flatten() {
                    if let Some(wanted) = wanted.as_str() {
                        let wanted = if wanted == "self.version" {
                            &package.version
                        } else {
                            wanted
                        };
                        pending.push((name.clone(), wanted.to_owned()));
                    }
                }
                for (name, _) in package.field("replace").as_object().into_iter().flatten() {
                    packages.remove(name);
                }
            }
        }
        Ok(Self { packages })
    }

    pub(super) fn package(&self, name: &str) -> Option<&Package> {
        self.packages.get(name)
    }

    pub(super) fn providers(&self, name: &str) -> impl Iterator<Item = &Package> {
        self.packages
            .values()
            .filter(move |package| offers(package, name))
    }
}

fn matches_candidate(package: &Package, wanted: &Constraint) -> bool {
    let Ok(version) = Version::parse(&package.version) else {
        return false;
    };
    let alias = package.field("extra")["branch-alias"][&package.version]
        .as_str()
        .and_then(|alias| Version::branch_alias(alias).ok());
    wanted.matches(&version) || alias.is_some_and(|alias| wanted.matches(&alias))
}

pub(crate) fn validate_request(names: &[String], previous: Option<&Lock>) -> Result<()> {
    let Some(previous) = previous else {
        bail!("A partial update requires composer.lock. Run `phera update` first.");
    };
    for name in names {
        if !previous
            .packages
            .iter()
            .chain(&previous.packages_dev)
            .any(|p| p.name == *name)
        {
            bail!("{name} is not in composer.lock.");
        }
    }
    Ok(())
}

fn offers(package: &Package, name: &str) -> bool {
    ["replace", "provide"]
        .into_iter()
        .any(|field| package.field(field).get(name).is_some())
}
