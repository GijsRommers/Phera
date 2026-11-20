//! A locked, content-addressed store: `PHERA_CACHE_DIR/objects/<sha256>/`.
//!
//! Vendor files use reflinks or private copies, so `post-install-cmd` cannot change the store.

use crate::{Package, network::Network};
use anyhow::{Context, Result, bail};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

mod inventory;

pub(crate) struct Store {
    pub(crate) directory: PathBuf,
}

impl Store {
    pub(crate) fn open(override_dir: Option<&Path>) -> Result<Self> {
        let directory = if let Some(path) = override_dir {
            path.to_path_buf()
        } else if let Some(path) = std::env::var_os("PHERA_CACHE_DIR") {
            PathBuf::from(path)
        } else if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
            PathBuf::from(path).join("phera/store")
        } else {
            let home =
                std::env::var_os("HOME").context("Set PHERA_CACHE_DIR for the package store.")?;
            let suffix = if cfg!(target_os = "macos") {
                "Library/Caches/phera/store"
            } else {
                ".cache/phera/store"
            };
            PathBuf::from(home).join(suffix)
        };
        for child in ["objects", "index", "locks", "staging", "metadata", "git"] {
            fs::create_dir_all(directory.join(child))?;
        }
        Ok(Self {
            directory: directory.canonicalize()?,
        })
    }

    pub(crate) fn prepare(
        &self,
        package: &Package,
        network: &Network,
        offline: bool,
        project: &Path,
    ) -> Result<PathBuf> {
        let dist = package.field("dist");
        let source = package.field("source");
        let mut identity = serde_json::to_vec(&serde_json::json!([
            package.name,
            crate::version::normalize(&package.version)?,
            dist["type"].as_str().unwrap_or("zip"),
            dist["url"].as_str(),
            dist["reference"].as_str(),
            dist["shasum"]
                .as_str()
                .filter(|sum| !sum.is_empty())
                .map(str::to_ascii_lowercase),
            source["type"].as_str(),
            source["url"].as_str(),
            source["reference"].as_str()
        ]))?;
        let path_dist = package.field("dist")["type"] == "path";
        if path_dist {
            identity.extend_from_slice(project.as_os_str().as_encoded_bytes());
        }
        let key = format!("{:x}", Sha256::digest(&identity));
        let guard = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("locks").join(&key))?;
        guard.lock()?;
        let index = self.directory.join("index").join(&key);
        if (!path_dist || offline)
            && let Some(object) = inventory::cached(&index, &self.directory.join("objects"))?
        {
            return Ok(object);
        }
        let source_only = package.field("dist").is_null() && source["type"] == "git";
        if offline && !source_only {
            bail!(
                "Offline store is missing {} {}. Install it online once.",
                package.name,
                package.version
            );
        }
        let stage = tempfile::tempdir_in(self.directory.join("staging"))?;
        let extracted = stage.path().join("package");
        fs::create_dir(&extracted)?;
        let dist = package.field("dist");
        let digest = if let Some(url) = dist["url"].as_str() {
            let kind = dist["type"].as_str().unwrap_or("zip");
            if kind == "path" {
                let source = crate::paths::inside(project, url)?.canonicalize()?;
                if !source.starts_with(project) {
                    bail!("Path repositories must stay inside the project directory.");
                }
                if self.directory.starts_with(&source) {
                    bail!(
                        "Move PHERA_CACHE_DIR outside this path repository before installing it."
                    );
                }
                copy_tree(&source, &extracted)?;
                tree_digest(&extracted)?
            } else {
                let bytes = if url.starts_with("file://") {
                    let source = Path::new(url.trim_start_matches("file://")).canonicalize()?;
                    if !source.starts_with(project) {
                        bail!("Local archives must stay inside the project directory.");
                    }
                    fs::read(source)?
                } else {
                    network.bytes(url)?
                };
                if let Some(expected) = dist["shasum"].as_str().filter(|sum| !sum.is_empty())
                    && format!("{:x}", Sha1::digest(&bytes)) != expected.to_ascii_lowercase()
                {
                    bail!(
                        "Checksum mismatch for {}; delete its cache and retry.",
                        package.name
                    );
                }
                crate::archive::extract(&bytes, kind, &extracted)
                    .with_context(|| format!("Cannot extract {}.", package.name))?;
                format!("{:x}", Sha256::digest(&bytes))
            }
        } else {
            crate::git::export(package, project, self, network, offline, &extracted)?;
            tree_digest(&extracted)?
        };
        let mut children = fs::read_dir(&extracted)?.collect::<std::io::Result<Vec<_>>>()?;
        let source = if children.len() == 1
            && children[0].file_type()?.is_dir()
            && dist["type"] != "path"
            && !dist.is_null()
        {
            children.remove(0).path()
        } else {
            extracted
        };
        for entry in walkdir::WalkDir::new(&source) {
            let entry = entry?;
            if entry.file_type().is_symlink() && !entry.path().canonicalize()?.starts_with(&source)
            {
                bail!(
                    "Archive symlink escapes the package after removing its top-level directory."
                );
            }
        }
        inventory::publish(&self.directory, &source, &index, &digest)
    }
}

pub(crate) fn materialise(
    source: &Path,
    destination: &Path,
    target_dir: Option<&str>,
) -> Result<&'static str> {
    let parent = destination
        .parent()
        .context("Package destination has no parent.")?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::tempdir_in(parent)?;
    let target = crate::paths::inside(stage.path(), target_dir.unwrap_or(""))?;
    fs::create_dir_all(&target)?;
    let method = copy_tree(source, &target)?;
    crate::path_install::remove(destination)?;
    fs::rename(stage.path(), destination)?;
    Ok(method)
}

pub(crate) fn copy_tree(source: &Path, target: &Path) -> Result<&'static str> {
    let mut method = "copy-on-write clone";
    for entry in walkdir::WalkDir::new(source)
        .min_depth(1)
        .sort_by_file_name()
    {
        let entry = entry?;
        let relative = entry.path().strip_prefix(source)?;
        let destination = crate::paths::inside(target, &relative.to_string_lossy())?;
        if entry.file_type().is_dir() {
            fs::create_dir_all(destination)?;
        } else if entry.file_type().is_symlink() {
            let link = fs::read_link(entry.path())?;
            let resolved = entry.path().canonicalize()?;
            if !resolved.starts_with(source) {
                bail!("Package contains a symlink outside its directory.");
            }
            #[cfg(unix)]
            std::os::unix::fs::symlink(link, destination)?;
            #[cfg(not(unix))]
            {
                let _ = link;
                fs::copy(resolved, destination)?;
            }
        } else if entry.file_type().is_file() {
            if reflink_copy::reflink(entry.path(), &destination).is_err() {
                fs::copy(entry.path(), &destination)?;
                method = "private copy (reflink unavailable)";
            }
            fs::set_permissions(destination, entry.metadata()?.permissions())?;
        } else {
            bail!("Package contains a special file; only regular files and internal links work.");
        }
    }
    Ok(method)
}

fn tree_digest(directory: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    for entry in walkdir::WalkDir::new(directory)
        .min_depth(1)
        .sort_by_file_name()
    {
        let entry = entry?;
        let path = entry
            .path()
            .strip_prefix(directory)?
            .as_os_str()
            .as_encoded_bytes();
        hash.update((path.len() as u64).to_le_bytes());
        hash.update(path);
        if entry.file_type().is_file() {
            hash.update(b"file");
            hash.update(entry.metadata()?.len().to_le_bytes());
            hash.update(fs::read(entry.path())?);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                hash.update((entry.metadata()?.permissions().mode() & 0o111).to_le_bytes());
            }
        } else if entry.file_type().is_symlink() {
            hash.update(b"link");
            hash.update(fs::read_link(entry.path())?.as_os_str().as_encoded_bytes());
        } else {
            hash.update(b"directory");
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}
