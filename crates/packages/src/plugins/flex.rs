//! Apply Symfony recipe data for packages in the lock, such as framework-bundle's Kernel.

mod files;

use crate::{Options, Package, Project, network::Network, paths, registry::cache, store::Store};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::Path;

pub(super) const NAME: &str = "symfony/flex";
const OFFICIAL: &str = "symfony/recipes";
const CONTRIB: &str = "symfony/recipes-contrib";

pub(super) fn apply(project: &Project, options: &Options, packages: &[Package]) -> Result<()> {
    let path = paths::inside(&project.directory, "symfony.lock")?;
    let mut lock = read_lock(&path)?;
    let previous = lock.clone();
    let store = Store::open(options.cache_dir.as_deref())?;
    let network = Network::new(project)?;
    let fetch = |url: &str| cache::fetch(&store, &network, url, options.offline);
    let mut index = None;
    let mut contrib = None;
    let mut outputs = files::Outputs::new(project)?;
    for package in packages {
        let name = package.name.as_str();
        let dotted = name.replace('/', ".");
        let pinned = &lock[name]["recipe"];
        if !pinned.is_null() && outputs.present(&lock[name])? {
            continue;
        }
        let version = major_minor(
            package.field("extra")["branch-alias"][&package.version]
                .as_str()
                .unwrap_or(&package.version),
        );
        let (repository, branch, recipe, url) = if !pinned.is_null() {
            let repository = pinned["repo"]
                .as_str()
                .and_then(|name| name.strip_prefix("github.com/"))
                .filter(|name| matches!(*name, OFFICIAL | CONTRIB))
                .with_context(|| format!("Unsupported recipe repository for {name}."))?;
            let branch = pinned["branch"].as_str().unwrap_or("main");
            let reference = pinned["ref"]
                .as_str()
                .context("Missing Symfony recipe ref.")?;
            if !reference.bytes().all(|byte| byte.is_ascii_hexdigit()) || reference.len() != 40 {
                bail!("Invalid Symfony recipe ref for {name}.");
            }
            if !matches!(branch, "main" | "master") {
                bail!("Unsupported recipe branch for {name}.");
            }
            let recipe = pinned["version"]
                .as_str()
                .context("Missing Symfony recipe version.")?;
            let url = recipe_url(
                repository,
                branch,
                &format!("archived/{dotted}/{reference}.json"),
            );
            (repository, branch, recipe, url)
        } else {
            let Some(version) = version else { continue };
            let official = select(recipe_index(&mut index, OFFICIAL, &fetch)?, name, version);
            let selected = if official.is_some() {
                official.map(|recipe| (OFFICIAL, recipe))
            } else if project.data["extra"]["symfony"]["allow-contrib"] == true {
                select(recipe_index(&mut contrib, CONTRIB, &fetch)?, name, version)
                    .map(|recipe| (CONTRIB, recipe))
            } else {
                None
            };
            let Some((repository, recipe)) = selected else {
                continue;
            };
            let url = recipe_url(repository, "main", &format!("{dotted}.{recipe}.json"));
            (repository, "main", recipe, url)
        };
        let body =
            fetch(&url).with_context(|| format!("Cannot fetch the Symfony recipe for {name}."))?;
        let manifest = &body["manifests"][name];
        let reference = manifest["ref"]
            .as_str()
            .context("Symfony recipe has no ref.")?;
        if let Some(expected) = pinned["ref"].as_str()
            && reference != expected
        {
            bail!("Symfony recipe ref does not match symfony.lock for {name}.");
        }
        let files = outputs.add(name, manifest)?;
        if pinned.is_null() {
            let (major, minor) = version.context("Missing numeric Symfony package version.")?;
            lock[name] = json!({
                "version": format!("{major}.{minor}"),
                "recipe": {
                    "repo": format!("github.com/{repository}"),
                    "branch": branch,
                    "version": recipe,
                    "ref": reference,
                },
                "files": files,
            });
        }
    }
    outputs.write()?;
    if previous != lock {
        let mut file = tempfile::NamedTempFile::new_in(&project.directory)?;
        serde_json::to_writer_pretty(&mut file, &lock)?;
        writeln!(file)?;
        file.persist(path)?;
    }
    Ok(())
}

fn read_lock(path: &Path) -> Result<Value> {
    match fs::read(path) {
        Ok(bytes) => {
            let lock: Value = serde_json::from_slice(&bytes).context("Invalid symfony.lock.")?;
            if !lock.is_object() {
                bail!("symfony.lock must contain a JSON object.");
            }
            Ok(lock)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(error) => Err(error.into()),
    }
}

fn recipe_url(repository: &str, branch: &str, file: &str) -> String {
    format!("https://raw.githubusercontent.com/{repository}/flex/{branch}/{file}")
}

fn recipe_index<'a>(
    index: &'a mut Option<Value>,
    repository: &str,
    fetch: &impl Fn(&str) -> Result<Value>,
) -> Result<&'a Value> {
    if index.is_none() {
        *index = Some(fetch(&recipe_url(repository, "main", "index.json"))?);
    }
    index.as_ref().context("Missing Symfony recipe index.")
}

fn select<'a>(index: &'a Value, name: &str, version: (u64, u64)) -> Option<&'a str> {
    index["recipes"][name]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter_map(|recipe| Some((major_minor(recipe)?, recipe)))
        .filter(|(candidate, _)| *candidate <= version)
        .max()
        .map(|(_, recipe)| recipe)
}

fn major_minor(version: &str) -> Option<(u64, u64)> {
    let (major, rest) = version.trim_start_matches('v').split_once('.')?;
    let minor = rest.split(['.', '-']).next()?;
    Some((
        major.parse().ok()?,
        if minor == "x" {
            9_999_999
        } else {
            minor.parse().ok()?
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipe_versions_follow_the_locked_package() {
        let index = json!({"recipes": {"symfony/framework-bundle": ["7.4", "7.10", "8.1"]}});
        let name = "symfony/framework-bundle";
        assert_eq!(select(&index, name, (7, 11)), Some("7.10"));
        assert_eq!(select(&index, name, (8, 0)), Some("7.10"));
        assert_eq!(select(&index, name, (8, 1)), Some("8.1"));
        assert_eq!(select(&index, name, (6, 4)), None);
        assert_eq!(major_minor("v8.1.11"), Some((8, 1)));
        assert_eq!(major_minor("7.x-dev"), Some((7, 9_999_999)));
    }
}
