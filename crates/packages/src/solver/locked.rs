//! Keep unrelated packages fixed during `phera update vendor/package`.

use crate::{Lock, Package};
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
        dependencies: bool,
    ) -> Result<Self> {
        if requested.is_empty() {
            return Ok(Self::default());
        }
        let mut packages: BTreeMap<_, _> = previous
            .into_iter()
            .flat_map(|lock| lock.packages(false))
            .map(|package| (package.name.clone(), package))
            .collect();
        let mut pending = requested.to_vec();
        let mut seen = BTreeSet::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(name.clone()) {
                continue;
            }
            let package = packages.remove(&name);
            if dependencies && let Some(package) = package {
                for requirement in package.field("require").as_object().into_iter().flatten() {
                    let (name, _) = requirement;
                    pending.push(name.clone());
                    pending.extend(
                        packages
                            .values()
                            .filter(|package| offers(package, name))
                            .map(|package| package.name.clone()),
                    );
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
