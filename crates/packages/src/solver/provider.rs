//! Adapt finite Composer package candidates to PubGrub version sets.
//!
//! A provider of `psr/log-implementation` selects the actual provider package as well.

use crate::{
    Options, Package, Project,
    registry::Registry,
    version::{Constraint, Version},
};
use anyhow::{Context, Result};
use pubgrub::{
    Dependencies, DependencyConstraints, DependencyProvider, PackageResolutionStatistics, Ranges,
};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::Arc;

use super::locked::Locked;

#[derive(Clone)]
pub(super) struct Candidate {
    pub(super) package: Package,
    pub(super) offered: Option<String>,
}

pub(super) struct Provider<'a> {
    pub(super) root: Package,
    pub(super) project: &'a Project,
    pub(super) options: &'a Options,
    pub(super) registry: &'a Registry,
    pub(super) locked: &'a Locked,
    pub(super) platform: BTreeMap<String, Version>,
    pub(super) catalogs: RefCell<BTreeMap<String, Arc<Vec<Candidate>>>>,
}

impl<'a> Provider<'a> {
    pub(super) fn new(
        project: &'a Project,
        options: &'a Options,
        registry: &'a Registry,
        locked: &'a Locked,
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
            version: project.root_version(),
            data,
        };
        let mut names = names(root.field("require"));
        names.retain(|name| locked.package(name).is_none());
        registry.prefetch(&names)?;
        Ok(Self {
            root,
            project,
            options,
            registry,
            locked,
            platform,
            catalogs: RefCell::new(BTreeMap::new()),
        })
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

    fn dependencies(
        &self,
        package: &Package,
    ) -> Result<DependencyConstraints<String, Ranges<u32>>> {
        let mut names = names(package.field("require"));
        names.retain(|name| self.locked.package(name).is_none());
        self.registry.prefetch(&names)?;
        let mut dependencies = BTreeMap::new();
        if package.name == self.root.name {
            dependencies.insert(self.root.name.clone(), Ranges::singleton(1u32));
        }
        for (field, negative) in [("require", false), ("conflict", true)] {
            if let Some(requirements) = package.field(field).as_object() {
                for (name, text) in requirements {
                    if super::is_platform(name) {
                        continue;
                    }
                    let text = text
                        .as_str()
                        .context("Dependency constraint must be a string.")?;
                    let text = if text == "self.version" {
                        &package.version
                    } else {
                        text
                    };
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
        if range.contains(&0) {
            return Ok(Some(0));
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
                for (platform, constraint) in requirements.iter().filter(|(name, _)| {
                    super::is_platform(name) && !self.options.ignores_platform(name)
                }) {
                    let constraint = constraint
                        .as_str()
                        .context("Platform constraint must be a string.")?;
                    let matched = self.platform.get(platform).is_some_and(|version| {
                        Constraint::parse(constraint)
                            .is_ok_and(|constraint| constraint.matches(version))
                    });
                    if matched == negative {
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
pub(super) struct SolveError(pub(super) anyhow::Error);

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
