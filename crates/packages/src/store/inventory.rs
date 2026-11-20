//! Detect incomplete stored objects before reusing them, including offline installs.
//!
//! If a cache cleaner deletes `objects/<digest>/src/Thing.php`, install downloads
//! a fresh object online or reports the package missing with `--offline`.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct Index {
    digest: String,
    entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
struct Entry {
    path: PathBuf,
    kind: Kind,
}

#[derive(Serialize, Deserialize, PartialEq, Eq)]
enum Kind {
    Directory,
    File { bytes: u64, executable: u32 },
    Link { target: PathBuf },
}

pub(super) fn cached(index: &Path, objects: &Path) -> Result<Option<PathBuf>> {
    let Ok(bytes) = fs::read(index) else {
        return Ok(None);
    };
    let Ok(record) = serde_json::from_slice::<Index>(&bytes) else {
        return Ok(None);
    };
    let object = crate::paths::inside(objects, &record.digest)?;
    if snapshot(&object).is_ok_and(|entries| entries == record.entries) {
        Ok(Some(object))
    } else {
        Ok(None)
    }
}

pub(super) fn publish(cache: &Path, source: &Path, index: &Path, digest: &str) -> Result<PathBuf> {
    let guard = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(cache.join("locks").join(format!("object-{digest}")))?;
    guard.lock()?;
    let entries = snapshot(source)?;
    let object = cache.join("objects").join(digest);
    if !snapshot(&object).is_ok_and(|stored| stored == entries) {
        crate::path_install::remove(&object)?;
        fs::rename(source, &object)?;
    }
    let record = Index {
        digest: digest.into(),
        entries,
    };
    let mut file = tempfile::NamedTempFile::new_in(cache.join("index"))?;
    file.write_all(&serde_json::to_vec(&record)?)?;
    file.persist(index)?;
    Ok(object)
}

fn snapshot(directory: &Path) -> Result<Vec<Entry>> {
    if !directory.is_dir() {
        anyhow::bail!("Stored package directory is missing.");
    }
    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(directory)
        .min_depth(1)
        .sort_by_file_name()
    {
        let entry = entry?;
        let kind = if entry.file_type().is_dir() {
            Kind::Directory
        } else if entry.file_type().is_symlink() {
            Kind::Link {
                target: fs::read_link(entry.path())?,
            }
        } else {
            let metadata = entry.metadata()?;
            let executable = {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o111
                }
                #[cfg(not(unix))]
                {
                    0
                }
            };
            Kind::File {
                bytes: metadata.len(),
                executable,
            }
        };
        entries.push(Entry {
            path: entry.path().strip_prefix(directory)?.into(),
            kind,
        });
    }
    Ok(entries)
}
