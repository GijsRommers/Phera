//! Extract untrusted zip/tar archives into a fresh private staging directory.
//!
//! An entry named `../../composer.json`, or a link outside the package, is rejected.

use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use std::fs::{self, File};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

const MAX_FILE: u64 = 256 * 1024 * 1024;
const MAX_PACKAGE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;

pub(crate) fn extract(bytes: &[u8], kind: &str, target: &Path) -> Result<()> {
    match kind {
        "zip" => unzip(bytes, target),
        "tar" => untar(Cursor::new(bytes), target),
        "tar.gz" | "tgz" | "gzip" => untar(GzDecoder::new(bytes), target),
        _ => bail!("Unsupported dist type {kind:?}; use zip or tar archives."),
    }
}

fn unzip(bytes: &[u8], target: &Path) -> Result<()> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    if archive.len() > MAX_ENTRIES {
        bail!("Archive has too many files (limit {MAX_ENTRIES}).");
    }
    let mut total = 0;
    let mut links = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let destination = crate::paths::inside(target, entry.name())?;
        let mode = entry.unix_mode().unwrap_or(0o644);
        check_size(entry.size(), &mut total)?;
        if mode & 0o170000 == 0o120000 {
            let mut link = String::new();
            entry.take(4097).read_to_string(&mut link)?;
            if link.len() > 4096 {
                bail!("Archive symlink target is too long.");
            }
            links.push((destination, PathBuf::from(link)));
        } else if entry.is_dir() {
            fs::create_dir_all(destination)?;
        } else {
            write_file(&mut entry, &destination, mode)?;
        }
    }
    write_links(target, links)
}

fn untar(reader: impl Read, target: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    let mut total = 0;
    let mut links = Vec::new();
    for (index, entry) in archive.entries()?.enumerate() {
        if index >= MAX_ENTRIES {
            bail!("Archive has too many files (limit {MAX_ENTRIES}).");
        }
        let mut entry = entry?;
        let name = entry.path()?.to_string_lossy().into_owned();
        let destination = crate::paths::inside(target, &name)?;
        check_size(entry.size(), &mut total)?;
        let kind = entry.header().entry_type();
        if kind.is_dir() {
            fs::create_dir_all(destination)?;
        } else if kind.is_file() {
            let mode = entry.header().mode()?;
            write_file(&mut entry, &destination, mode)?;
        } else if kind.is_symlink() {
            let link = entry
                .link_name()?
                .context("Archive symlink has no target.")?
                .into_owned();
            links.push((destination, link));
        } else {
            bail!("Archive contains a device, hard link or unsupported special entry.");
        }
    }
    write_links(target, links)
}

fn check_size(size: u64, total: &mut u64) -> Result<()> {
    *total = total.checked_add(size).context("Archive size overflow.")?;
    if size > MAX_FILE || *total > MAX_PACKAGE {
        bail!("Archive exceeds safety limits (256 MiB per file, 2 GiB unpacked).");
    }
    Ok(())
}

fn write_file(reader: &mut impl Read, destination: &Path, mode: u32) -> Result<()> {
    fs::create_dir_all(
        destination
            .parent()
            .context("Missing archive parent directory.")?,
    )?;
    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let written = std::io::copy(&mut reader.take(MAX_FILE + 1), &mut file)?;
    if written > MAX_FILE {
        bail!("Archive file exceeds its safety limit.");
    }
    set_mode(destination, mode)
}

pub(crate) fn set_mode(path: &Path, mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644 | (mode & 0o111)))?;
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
    Ok(())
}

fn write_links(root: &Path, links: Vec<(PathBuf, PathBuf)>) -> Result<()> {
    // Create links last. No archive file can subsequently write through one.
    for (destination, link) in &links {
        if link.is_absolute() || link.to_string_lossy().contains(['\\', ':']) {
            bail!("Archive symlink points outside the package.");
        }
        let mut resolved = destination
            .parent()
            .context("Missing link parent.")?
            .to_path_buf();
        for component in link.components() {
            match component {
                std::path::Component::ParentDir => {
                    resolved.pop();
                    if !resolved.starts_with(root) {
                        bail!("Archive symlink points outside the package.");
                    }
                }
                std::path::Component::Normal(name) => resolved.push(name),
                std::path::Component::CurDir => {}
                _ => bail!("Invalid archive symlink."),
            }
        }
        if links.iter().any(|(other, _)| resolved.starts_with(other)) {
            bail!("Archive symlink chains are not supported; publish direct internal links.");
        }
    }
    for (destination, link) in links {
        fs::create_dir_all(destination.parent().context("Missing link parent.")?)?;
        #[cfg(unix)]
        std::os::unix::fs::symlink(link, destination)?;
        #[cfg(not(unix))]
        {
            let _ = (destination, link);
            bail!("Archive symlinks require a Unix filesystem.");
        }
    }
    Ok(())
}
