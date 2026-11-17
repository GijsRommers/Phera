//! A locked, content-addressed store: `PHERA_CACHE_DIR/objects/<sha256>/`.
//!
//! Vendor files use reflinks or private copies, so `post-install-cmd` cannot change the store.

use crate::{Package, network::Network};
use anyhow::{Context, Result, bail};
use fs4::fs_std::FileExt;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
        for child in ["objects", "index", "locks", "staging", "metadata"] {
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
        let identity = serde_json::to_vec(package)?;
        let key = format!("{:x}", Sha256::digest(&identity));
        let guard = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("locks").join(&key))?;
        guard.lock_exclusive()?;
        let index = self.directory.join("index").join(&key);
        if let Ok(digest) = fs::read_to_string(&index) {
            let object = crate::paths::inside(&self.directory.join("objects"), digest.trim())?;
            if object.is_dir() {
                return Ok(object);
            }
        }
        if offline {
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
            clone_source(package, &extracted)?;
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
        let object = self.directory.join("objects").join(&digest);
        if !object.exists() {
            match fs::rename(&source, &object) {
                Ok(()) => {}
                Err(_) if object.is_dir() => {}
                Err(error) => return Err(error.into()),
            }
        }
        let mut record = tempfile::NamedTempFile::new_in(self.directory.join("index"))?;
        use std::io::Write;
        record.write_all(digest.as_bytes())?;
        record.persist(index)?;
        Ok(object)
    }
}

fn clone_source(package: &Package, target: &Path) -> Result<()> {
    let source = package.field("source");
    if source["type"] != "git" {
        bail!("{} has no dist or supported git source.", package.name);
    }
    let url = source["url"]
        .as_str()
        .context("Git source is missing its URL.")?;
    let reference = source["reference"]
        .as_str()
        .context("Git source is missing its reference.")?;
    if !url.starts_with("https://") || url.contains('@') || reference.starts_with('-') {
        bail!("Use an HTTPS git source without credentials embedded in the URL.");
    }
    let success = Command::new("git")
        .args(["clone", "--no-checkout", "--", url])
        .arg(target)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?
        .success();
    if !success
        || !Command::new("git")
            .arg("-C")
            .arg(target)
            .args(["checkout", "--detach", reference])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?
            .success()
    {
        bail!(
            "Cannot check out git source for {}; check access and reference.",
            package.name
        );
    }
    fs::remove_dir_all(target.join(".git"))?;
    Ok(())
}

pub(crate) fn materialise(source: &Path, destination: &Path) -> Result<&'static str> {
    let parent = destination
        .parent()
        .context("Package destination has no parent.")?;
    fs::create_dir_all(parent)?;
    let stage = tempfile::tempdir_in(parent)?;
    let method = copy_tree(source, stage.path())?;
    if destination.exists() {
        fs::remove_dir_all(destination)?;
    }
    fs::rename(stage.path(), destination)?;
    Ok(method)
}

fn copy_tree(source: &Path, target: &Path) -> Result<&'static str> {
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
