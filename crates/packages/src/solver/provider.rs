//! Adapt finite Composer package candidates to PubGrub version sets.
//!
//! A provider of `psr/log-implementation` selects the actual provider package as well.

use crate::{
    Options, Package, Project,
    registry::Registry,
    version::{Constraint, Stability, Version},
};
use anyhow::{Context, Result};
use pubgrub::{
    Dependencies, DependencyConstraints, DependencyProvider, PackageResolutionStatistics, Ranges,
};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Clone)]
struct Candidate {
    package: Package,
    offered: Option<String>,
}

pub(super) struct Provider<'a> {
    root: Package,
    project: &'a Project,
    options: &'a Options,
    registry: &'a Registry,
    platform: BTreeMap<String, Version>,
    catalogs: RefCell<BTreeMap<String, Vec<Candidate>>>,
}

impl<'a> Provider<'a> {
    pub(super) fn new(
        project: &'a Project,
        options: &'a Options,
        registry: &'a Registry,
        platform: BTreeMap<String, Version>,
    ) -> Result<Self> {
        let mut data = project.data.as_object().cloned().unwrap_or_default();
        let mut requirements = data
            .get("require")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        if let (Some(target), Some(extra)) = (
            requirements.as_object_mut(),
            data.get("require-dev").and_then(Value::as_object),
        ) {
            target.extend(extra.clone());
        }
        data.insert("require".into(), requirements);
        let root = Package {
            name: project.name().into(),
            version: project.data["version"].as_str().unwrap_or("1.0.0").into(),
            data,
        };
        let names = names(root.field("require"));
        registry.prefetch(&names)?;
        Ok(Self {
            root,
            project,
            options,
            registry,
            platform,
            catalogs: RefCell::new(BTreeMap::new()),
        })
    }

    fn catalog(&self, name: &str) -> Result<Vec<Candidate>> {
        if let Some(catalog) = self.catalogs.borrow().get(name) {
            return Ok(catalog.clone());
        }
        let mut candidates = self
            .registry
            .packages(name)?
            .into_iter()
            .filter(|package| self.allowed(package))
            .map(|package| Candidate {
                package,
                offered: None,
            })
            .collect::<Vec<_>>();
        let mut providers = self.registry.providers(name)?;
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
        candidates.sort_by(|left, right| {
            let left = Version::parse(&left.package.version).ok();
            let right = Version::parse(&right.package.version).ok();
            let stable = if self.project.data["prefer-stable"] == true {
                left.as_ref()
                    .map(Version::stability)
                    .cmp(&right.as_ref().map(Version::stability))
            } else {
                std::cmp::Ordering::Equal
            };
            stable.then_with(|| {
                let order = left.cmp(&right);
                if self.options.prefer_lowest {
                    order.reverse()
                } else {
                    order
                }
            })
        });
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

    pub(super) fn package(&self, name: &str, id: u32) -> Result<Option<Package>> {
        if id == 0 {
            return Ok(None);
        }
        Ok(self
            .catalog(name)?
            .get(id as usize - 1)
            .filter(|candidate| candidate.offered.is_none())
            .map(|candidate| candidate.package.clone()))
    }

    fn matching(&self, name: &str, text: &str, negative: bool) -> Result<Ranges<u32>> {
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
                normal
                    || aliases.is_some_and(|alias| {
                        Version::parse(alias).is_ok_and(|version| constraint.matches(&version))
                    })
            };
            if matched != negative {
                range = range.union(&Ranges::singleton(index as u32 + 1));
            }
        }
        Ok(range)
    }

    fn dependencies(
        &self,
        package: &Package,
    ) -> Result<DependencyConstraints<String, Ranges<u32>>> {
        self.registry.prefetch(&names(package.field("require")))?;
        let mut dependencies = BTreeMap::new();
        for (field, negative) in [("require", false), ("conflict", true)] {
            if let Some(requirements) = package.field(field).as_object() {
                for (name, text) in requirements {
                    if super::is_platform(name) {
                        continue;
                    }
                    let text = text
                        .as_str()
                        .context("Dependency constraint must be a string.")?;
                    let range = self.matching(name, text, negative)?;
                    dependencies
                        .entry(name.clone())
                        .and_modify(|old: &mut Ranges<u32>| {
                            *old = old.intersection(&range);
                        })
                        .or_insert(range);
                }
            }
        }
        if let Some(replaced) = package.field("replace").as_object() {
            for name in replaced.keys().filter(|name| !super::is_platform(name)) {
                let catalog = self.catalog(name)?;
                let mut range = Ranges::singleton(0u32);
                for (index, candidate) in catalog.iter().enumerate() {
                    if candidate.offered.is_some()
                        && candidate.package.name == package.name
                        && candidate.package.version == package.version
                    {
                        range = range.union(&Ranges::singleton(index as u32 + 1));
                    }
                }
                dependencies
                    .entry(name.clone())
                    .and_modify(|old| *old = old.intersection(&range))
                    .or_insert(range);
            }
        }
        Ok(dependencies.into_iter().collect())
    }
}

impl DependencyProvider for Provider<'_> {
    type P = String;
    type V = u32;
    type VS = Ranges<u32>;
    type M = String;
    type Err = SolveError;
    type Priority = (u32, std::cmp::Reverse<usize>);

    fn choose_version(
        &self,
        name: &String,
        range: &Ranges<u32>,
    ) -> std::result::Result<Option<u32>, SolveError> {
        if name == "@root" {
            return Ok(Some(1));
        }
        let count = self.catalog(name)?.len() as u32;
        Ok((0..=count).rev().find(|version| range.contains(version)))
    }

    fn prioritize(
        &self,
        name: &String,
        range: &Ranges<u32>,
        stats: &PackageResolutionStatistics,
    ) -> Self::Priority {
        let count = self.catalogs.borrow().get(name).map_or(0, |catalog| {
            (0..=catalog.len() as u32)
                .filter(|id| range.contains(id))
                .count()
        });
        (stats.conflict_count(), std::cmp::Reverse(count))
    }

    fn get_dependencies(
        &self,
        name: &String,
        id: &u32,
    ) -> std::result::Result<Dependencies<String, Ranges<u32>, String>, SolveError> {
        if *id == 0 {
            return Ok(Dependencies::Available(Default::default()));
        }
        let candidate = if name == "@root" {
            Candidate {
                package: self.root.clone(),
                offered: None,
            }
        } else {
            self.catalog(name)?
                .get(*id as usize - 1)
                .context("Invalid solver candidate.")?
                .clone()
        };
        let package = &candidate.package;
        if candidate.offered.is_some() {
            let actual = if package.name == self.root.name {
                "@root"
            } else {
                &package.name
            };
            let range = if actual == "@root" {
                Ranges::singleton(1u32)
            } else {
                self.matching(actual, &package.version, false)?
            };
            return Ok(Dependencies::Available(
                [(actual.to_owned(), range)].into_iter().collect(),
            ));
        }
        for (field, negative) in [("require", false), ("conflict", true)] {
            if let Some(requirements) = package.field(field).as_object() {
                for (platform, constraint) in requirements
                    .iter()
                    .filter(|(name, _)| super::is_platform(name))
                {
                    let constraint = constraint
                        .as_str()
                        .context("Platform constraint must be a string.")?;
                    let matched = self.platform.get(platform).is_some_and(|version| {
                        Constraint::parse(constraint)
                            .is_ok_and(|constraint| constraint.matches(version))
                    });
                    if matched == negative && !self.options.ignore_platform {
                        return Ok(Dependencies::Unavailable(format!(
                            "{} {} needs {platform} {constraint}; \
                            configure config.platform or use a Phera binary providing it",
                            package.name, package.version
                        )));
                    }
                }
            }
        }
        Ok(Dependencies::Available(self.dependencies(package)?))
    }
}

fn names(requirements: &Value) -> Vec<String> {
    requirements
        .as_object()
        .map(|items| {
            items
                .keys()
                .filter(|name| !super::is_platform(name))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug)]
pub(super) struct SolveError(anyhow::Error);

impl std::fmt::Display for SolveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:#}", self.0)
    }
}

impl std::error::Error for SolveError {}
impl From<anyhow::Error> for SolveError {
    fn from(error: anyhow::Error) -> Self {
        Self(error)
    }
}
