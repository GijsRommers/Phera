//! Lossless Composer manifests and locks: `{ "require": { "psr/log": "^3" } }`.
//!
//! Unknown package fields survive lock/installed metadata round trips.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub no_dev: bool,
    pub frozen: bool,
    pub offline: bool,
    pub optimize: bool,
    pub verbose: bool,
    pub no_scripts: bool,
    pub prefer_lowest: bool,
    pub ignore_platform: bool,
    pub cache_dir: Option<PathBuf>,
    pub platform: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct Project {
    pub directory: PathBuf,
    pub data: Value,
}

impl Project {
    pub fn read(directory: &Path) -> Result<Self> {
        let directory = directory.canonicalize()?;
        let path = directory.join("composer.json");
        let data: Value = serde_json::from_slice(
            &fs::read(&path).context("Missing composer.json. Run this in your PHP project.")?,
        )
        .context("Invalid composer.json; fix its JSON syntax.")?;
        if !data.is_object() {
            bail!("composer.json must contain a JSON object.");
        }
        Ok(Self { directory, data })
    }

    pub fn vendor_dir(&self) -> Result<PathBuf> {
        crate::paths::inside(&self.directory, self.config_str("vendor-dir", "vendor"))
    }

    pub fn bin_dir(&self) -> Result<PathBuf> {
        match self.data["config"]["bin-dir"].as_str() {
            Some(path) => crate::paths::inside(&self.directory, path),
            None => Ok(self.vendor_dir()?.join("bin")),
        }
    }

    pub fn config_str<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.data["config"][key].as_str().unwrap_or(default)
    }

    pub fn config_bool(&self, key: &str) -> bool {
        self.data["config"][key].as_bool().unwrap_or(false)
    }

    pub fn name(&self) -> &str {
        self.data["name"].as_str().unwrap_or("__root__")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Lock {
    #[serde(rename = "content-hash")]
    pub content_hash: String,
    pub packages: Vec<Package>,
    #[serde(default, rename = "packages-dev")]
    pub packages_dev: Vec<Package>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, Value>,
}

impl Lock {
    pub fn read(directory: &Path) -> Result<Self> {
        serde_json::from_slice(
            &fs::read(directory.join("composer.lock")).context(
                "No composer.lock found. Run `phera update` to resolve dependencies first.",
            )?,
        )
        .context("Invalid composer.lock. Restore it or run `phera update`.")
    }

    pub fn packages(&self, no_dev: bool) -> Vec<Package> {
        let mut packages = self.packages.clone();
        if !no_dev {
            packages.extend(self.packages_dev.iter().cloned());
        }
        packages.sort_by(|left, right| left.name.cmp(&right.name));
        packages
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    #[serde(flatten)]
    pub data: serde_json::Map<String, Value>,
}

impl Package {
    pub fn field(&self, name: &str) -> &Value {
        self.data.get(name).unwrap_or(&Value::Null)
    }

    pub fn kind(&self) -> &str {
        self.field("type").as_str().unwrap_or("library")
    }

    pub fn validate(&self) -> Result<()> {
        let parts: Vec<_> = self.name.split('/').collect();
        if parts.len() != 2
            || parts.iter().any(|part| {
                part.is_empty()
                    || part.starts_with('.')
                    || !part.chars().all(|character| {
                        character.is_ascii_alphanumeric() || "_-.".contains(character)
                    })
            })
        {
            bail!(
                "Invalid package name {:?}; expected vendor/package.",
                self.name
            );
        }
        Ok(())
    }
}

pub(crate) fn strings(value: &Value) -> Vec<&str> {
    match value {
        Value::String(text) => vec![text],
        Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    }
}
