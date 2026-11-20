//! Build Composer candidate domains and match provides, replaces and branch aliases.
//!
//! `dev-main as 2.0` lets a branch satisfy a transitive `^2` constraint.

use super::provider::{Candidate, Provider};
use crate::Package;
use crate::version::{Constraint, Stability, Version};
use anyhow::Result;
use pubgrub::Ranges;
use serde_json::Value;
use std::sync::Arc;

impl Provider<'_> {
    pub(super) fn catalog(&self, name: &str) -> Result<Arc<Vec<Candidate>>> {
        if let Some(catalog) = self.catalogs.borrow().get(name) {
            return Ok(catalog.clone());
        }
        if name == self.root.name {
            let catalog = Arc::new(vec![Candidate {
                package: self.root.clone(),
                offered: Some(self.root.version.clone()),
            }]);
            self.catalogs
                .borrow_mut()
                .insert(name.into(), catalog.clone());
            return Ok(catalog);
        }
        let packages = match self.locked.package(name) {
            Some(package) => vec![package.clone()],
            None => self.registry.packages(name)?,
        };
        let mut candidates = packages
            .into_iter()
            .filter(|package| self.allowed(package))
            .map(|package| Candidate {
                package,
                offered: None,
            })
            .collect::<Vec<_>>();
        let mut providers = self.registry.providers(name)?;
        providers.retain(|package| self.locked.package(&package.name).is_none());
        providers.extend(self.locked.providers(name).cloned());
        if self.root.field("replace").get(name).is_some()
            || self.root.field("provide").get(name).is_some()
        {
            providers.push(self.root.clone());
        }
        for package in providers
            .into_iter()
            .filter(|package| self.allowed(package))
        {
            let offer = package
                .field("replace")
                .get(name)
                .or_else(|| package.field("provide").get(name))
                .and_then(Value::as_str);
            if let Some(offer) = offer {
                let offer = if offer == "self.version" {
                    package.version.clone()
                } else {
                    offer.into()
                };
                candidates.push(Candidate {
                    package,
                    offered: Some(offer),
                });
            }
        }
        let mut ranked: Vec<_> = candidates
            .into_iter()
            .map(|candidate| (preference(&candidate.package), candidate))
            .collect();
        ranked.sort_by(|(left, _), (right, _)| {
            let stable = if self.project.data["prefer-stable"] == true {
                left.as_ref()
                    .map(Version::stability)
                    .cmp(&right.as_ref().map(Version::stability))
            } else {
                std::cmp::Ordering::Equal
            };
            stable.then_with(|| {
                let order = left.cmp(right);
                if self.options.prefer_lowest {
                    order.reverse()
                } else {
                    order
                }
            })
        });
        let candidates = Arc::new(
            ranked
                .into_iter()
                .map(|(_, candidate)| candidate)
                .collect::<Vec<_>>(),
        );
        self.catalogs
            .borrow_mut()
            .insert(name.into(), candidates.clone());
        Ok(candidates)
    }

    fn allowed(&self, package: &Package) -> bool {
        let minimum = self.project.data["minimum-stability"]
            .as_str()
            .and_then(Stability::parse)
            .unwrap_or(Stability::Stable);
        let root = self
            .root
            .field("require")
            .get(&package.name)
            .and_then(Value::as_str);
        let explicit = root
            .and_then(|constraint| {
                constraint
                    .rsplit_once('@')
                    .and_then(|(_, flag)| Stability::parse(flag))
            })
            .or_else(|| {
                root.filter(|constraint| constraint.contains("dev"))
                    .map(|_| Stability::Dev)
            });
        Version::parse(&package.version)
            .is_ok_and(|version| version.stability() >= explicit.unwrap_or(minimum))
    }

    pub(super) fn matching(&self, name: &str, text: &str, negative: bool) -> Result<Ranges<u32>> {
        let constraint = Constraint::parse(text)?;
        let mut range = if negative {
            Ranges::singleton(0u32)
        } else {
            Ranges::empty()
        };
        for (index, candidate) in self.catalog(name)?.iter().enumerate() {
            let matched = if let Some(offer) = &candidate.offered {
                constraint.intersects(&Constraint::parse(offer)?)
            } else {
                let normal = constraint.matches(&Version::parse(&candidate.package.version)?);
                let aliases = candidate.package.field("extra")["branch-alias"]
                    .get(&candidate.package.version)
                    .and_then(Value::as_str);
                let inline = self
                    .root
                    .field("require")
                    .get(&candidate.package.name)
                    .and_then(Value::as_str)
                    .and_then(|text| text.split_once(" as "))
                    .filter(|(source, _)| {
                        Version::parse(source).ok()
                            == Version::parse(&candidate.package.version).ok()
                    })
                    .is_some_and(|(_, alias)| {
                        Version::parse(alias).is_ok_and(|version| constraint.matches(&version))
                    });
                normal
                    || inline
                    || aliases.is_some_and(|alias| {
                        Version::branch_alias(alias)
                            .is_ok_and(|version| constraint.matches(&version))
                    })
            };
            if matched != negative {
                range = range.union(&Ranges::singleton(index as u32 + 1));
            }
        }
        Ok(range)
    }
}

fn preference(package: &Package) -> Option<Version> {
    if let Some(alias) = package.field("extra")["branch-alias"]
        .get(&package.version)
        .and_then(Value::as_str)
    {
        return Version::branch_alias(alias).ok();
    }
    if package.field("default-branch") == true && package.version.starts_with("dev-") {
        return Version::parse("9999999-dev").ok();
    }
    Version::parse(&package.version).ok()
}
