//! Composer v2 repositories and Packagist minified metadata.
//!
//! `psr/log` is fetched from `/p2/psr/log.json` and expanded before solving.

mod cache;
mod local;

use crate::{Options, Package, Project, network::Network, store::Store};
use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Mutex;

enum Repository {
    Composer {
        base: String,
        index: Value,
        canonical: bool,
    },
    Local {
        packages: Vec<Package>,
        canonical: bool,
    },
}

pub(crate) struct Registry {
    repositories: Vec<Repository>,
    network: Network,
    store: Store,
    offline: bool,
    dev: bool,
    known: Mutex<BTreeMap<String, Vec<Package>>>,
}

impl Registry {
    pub(crate) fn new(project: &Project, options: &Options) -> Result<Self> {
        let store = Store::open(options.cache_dir.as_deref())?;
        let network = Network::new(&project.directory)?;
        let mut repositories = Vec::new();
        let mut packagist = true;
        let configurations = match &project.data["repositories"] {
            Value::Array(items) => items.iter().collect::<Vec<_>>(),
            Value::Object(items) => {
                if items.get("packagist.org") == Some(&Value::Bool(false))
                    || items.get("packagist") == Some(&Value::Bool(false))
                {
                    packagist = false;
                }
                items.values().collect()
            }
            _ => Vec::new(),
        };
        for configuration in configurations {
            if configuration.get("packagist.org") == Some(&Value::Bool(false))
                || configuration.get("packagist") == Some(&Value::Bool(false))
            {
                packagist = false;
                continue;
            }
            if configuration == &Value::Bool(false) {
                continue;
            }
            let canonical = configuration["canonical"].as_bool().unwrap_or(true);
            match configuration["type"].as_str() {
                Some("package") => {
                    let entries = match &configuration["package"] {
                        Value::Array(items) => items.clone(),
                        value => vec![value.clone()],
                    };
                    let packages = entries
                        .into_iter()
                        .map(serde_json::from_value)
                        .collect::<std::result::Result<Vec<Package>, _>>()?;
                    repositories.push(Repository::Local {
                        packages,
                        canonical,
                    });
                }
                Some("path" | "artifact") => repositories.push(Repository::Local {
                    packages: local::packages(project, configuration, &store, &network)?,
                    canonical,
                }),
                Some("composer") => {
                    let base = configuration["url"]
                        .as_str()
                        .context("Composer repository needs url.")?
                        .trim_end_matches('/')
                        .to_owned();
                    let index = cache::fetch(
                        &store,
                        &network,
                        &format!("{base}/packages.json"),
                        options.offline,
                    )?;
                    repositories.push(Repository::Composer {
                        base,
                        index,
                        canonical,
                    });
                }
                Some("vcs" | "git" | "github") => {
                    bail!(
                        "VCS repositories are not supported yet; use a Composer or path repository."
                    );
                }
                Some(kind) => bail!("Unsupported repository type {kind:?}."),
                None => {}
            }
        }
        if packagist {
            repositories.push(Repository::Composer {
                base: "https://repo.packagist.org".into(),
                index: serde_json::json!({"metadata-url": "/p2/%package%.json"}),
                canonical: true,
            });
        }
        let dev = project.data["minimum-stability"]
            .as_str()
            .is_some_and(|value| value != "stable")
            || ["require", "require-dev"].iter().any(|key| {
                project.data[*key].as_object().is_some_and(|items| {
                    items.values().any(|constraint| {
                        constraint
                            .as_str()
                            .is_some_and(|text| text.contains('@') || text.contains("dev"))
                    })
                })
            });
        Ok(Self {
            repositories,
            network,
            store,
            offline: options.offline,
            dev,
            known: Mutex::new(BTreeMap::new()),
        })
    }

    pub(crate) fn packages(&self, name: &str) -> Result<Vec<Package>> {
        Package {
            name: name.into(),
            version: "1.0.0".into(),
            data: Default::default(),
        }
        .validate()?;
        if let Some(known) = self
            .known
            .lock()
            .map_err(|_| anyhow::anyhow!("Registry lock failed."))?
            .get(name)
            .cloned()
        {
            return Ok(known);
        }
        let mut packages = Vec::new();
        for repository in &self.repositories {
            let (found, canonical) = match repository {
                Repository::Local {
                    packages,
                    canonical,
                } => (
                    packages
                        .iter()
                        .filter(|package| package.name == name)
                        .cloned()
                        .collect(),
                    canonical,
                ),
                Repository::Composer {
                    base,
                    index,
                    canonical,
                } => {
                    let mut found = Vec::new();
                    if let Some(template) = index["metadata-url"].as_str() {
                        let mut names = vec![name.to_owned()];
                        if self.dev {
                            names.push(format!("{name}~dev"));
                        }
                        let replies: Vec<Value> = names
                            .par_iter()
                            .map(|name| {
                                let endpoint = template.replace("%package%", name);
                                let url = reqwest::Url::parse(&format!("{base}/"))?
                                    .join(&endpoint)?
                                    .to_string();
                                cache::fetch(&self.store, &self.network, &url, self.offline)
                            })
                            .collect::<Result<_>>()?;
                        for reply in replies {
                            found.extend(expand(&reply, name)?);
                        }
                    } else {
                        found.extend(expand(index, name)?);
                    }
                    (found, canonical)
                }
            };
            let found_any = !found.is_empty();
            packages.extend(found);
            if found_any && *canonical {
                break;
            }
        }
        self.known
            .lock()
            .map_err(|_| anyhow::anyhow!("Registry lock failed."))?
            .insert(name.to_owned(), packages.clone());
        Ok(packages)
    }

    pub(crate) fn prefetch(&self, names: &[String]) -> Result<()> {
        names
            .par_iter()
            .try_for_each(|name| self.packages(name).map(|_| ()))
    }

    pub(crate) fn providers(&self, name: &str) -> Result<Vec<Package>> {
        let mut all: Vec<Package> = self
            .known
            .lock()
            .map_err(|_| anyhow::anyhow!("Registry lock failed."))?
            .values()
            .flatten()
            .cloned()
            .collect();
        for repository in &self.repositories {
            if let Repository::Local { packages, .. } = repository {
                all.extend(packages.clone());
            }
        }
        Ok(all
            .into_iter()
            .filter(|package| {
                ["provide", "replace"]
                    .iter()
                    .any(|key| package.field(key).get(name).is_some())
            })
            .collect())
    }
}

fn expand(metadata: &Value, name: &str) -> Result<Vec<Package>> {
    let versions = &metadata["packages"][name];
    let entries: Vec<Value> = match versions {
        Value::Array(items) => items.clone(),
        Value::Object(items) => items.values().cloned().collect(),
        _ => return Ok(Vec::new()),
    };
    let minified = metadata["minified"] == "composer/2.0";
    let mut expanded = serde_json::Map::new();
    let mut packages = Vec::new();
    for entry in entries {
        let data = entry
            .as_object()
            .context("Package metadata must be an object.")?;
        if !minified {
            expanded.clear();
        }
        for (key, value) in data {
            if value == "__unset" {
                expanded.remove(key);
            } else {
                expanded.insert(key.clone(), value.clone());
            }
        }
        let package: Package = serde_json::from_value(Value::Object(expanded.clone()))?;
        package.validate()?;
        if package.name != name {
            bail!("Repository returned a different package name than requested.");
        }
        packages.push(package);
    }
    Ok(packages)
}
