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
    pub authoritative: bool,
    pub verbose: bool,
    pub no_scripts: bool,
    pub no_plugins: bool,
    pub no_interaction: bool,
    pub no_install: bool,
    pub dry_run: bool,
    pub refresh_lock: bool,
    pub with_all_dependencies: bool,
    pub audit_summary: bool,
    pub outdated_all: bool,
    pub prefer_lowest: bool,
    pub ignore_platform: bool,
    pub ignored_platform_requirements: Vec<glob::Pattern>,
    pub cache_dir: Option<PathBuf>,
    pub platform: std::collections::BTreeMap<String, String>,
}

impl Options {
    pub fn ignore_platform_requirement(&mut self, name: &str) -> Result<()> {
        if name.is_empty() {
            bail!("--ignore-platform-req needs a platform package name.");
        }
        self.ignored_platform_requirements
            .push(glob::Pattern::new(name)?);
        Ok(())
    }

    pub(crate) fn ignores_platform(&self, name: &str) -> bool {
        self.ignore_platform
            || self
                .ignored_platform_requirements
                .iter()
                .any(|p| p.matches(name))
    }

    pub(crate) fn authoritative(&self, project: &Project) -> bool {
        self.authoritative || project.config_bool("classmap-authoritative")
    }
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

    pub(crate) fn validate_output_paths(&self) -> Result<()> {
        let vendor = self.vendor_dir()?;
        crate::paths::inside(&vendor, "autoload.php")?;
        let composer = crate::paths::inside(&vendor, "composer")?;
        for name in [
            "ClassLoader.php",
            "InstalledVersions.php",
            "LICENSE",
            "installed.json",
            "installed.php",
            "autoload_psr4.php",
            "autoload_namespaces.php",
            "autoload_classmap.php",
            "autoload_files.php",
            "autoload_static.php",
            "autoload_real.php",
            "platform_check.php",
            "include_paths.php",
        ] {
            crate::paths::inside(&composer, name)?;
        }
        Ok(())
    }

    pub fn config_str<'a>(&'a self, key: &str, default: &'a str) -> &'a str {
        self.data["config"][key].as_str().unwrap_or(default)
    }

    pub fn config_bool(&self, key: &str) -> bool {
        self.data["config"][key].as_bool().unwrap_or(false)
    }

    pub fn root_version(&self) -> String {
        self.data["version"]
            .as_str()
            .map(str::to_owned)
            .or_else(|| std::env::var("COMPOSER_ROOT_VERSION").ok())
            .unwrap_or_else(|| "1.0.0+no-version-set".into())
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
    pub fn install_path(&self, vendor: &Path) -> Result<PathBuf> {
        let base = crate::path_install::destination(vendor, self)?;
        match self.field("target-dir").as_str() {
            Some(target) => crate::paths::inside(&base, target),
            None => Ok(base),
        }
    }

    pub fn field(&self, name: &str) -> &Value {
        self.data.get(name).unwrap_or(&Value::Null)
    }

    pub fn kind(&self) -> &str {
        self.field("type").as_str().unwrap_or("library")
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(target) = self.field("target-dir").as_str() {
            crate::paths::relative(target)?;
        }
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
        for field in ["source", "dist"] {
            if let Some(address) = self.field(field)["url"].as_str()
                && let Ok(url) = reqwest::Url::parse(address)
                && (url.password().is_some()
                    || matches!(url.scheme(), "http" | "https") && !url.username().is_empty())
            {
                bail!(
                    "{} has credentials in its {field} URL. Use auth.json instead.",
                    self.name
                );
            }
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
