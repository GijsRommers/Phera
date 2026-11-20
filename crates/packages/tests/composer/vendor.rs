//! Compare every vendor path, file hash, executable bit and symbolic link.

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[derive(PartialEq, Eq)]
enum Entry {
    Directory(u32),
    Link(PathBuf),
    File { sha256: String, executable: u32 },
}

fn tree(root: &Path) -> Result<BTreeMap<PathBuf, Entry>> {
    let mut tree = BTreeMap::new();
    for entry in walkdir::WalkDir::new(root).min_depth(1) {
        let entry = entry?;
        let path = entry.path();
        let value = if entry.file_type().is_dir() {
            Entry::Directory(mode(&entry.metadata()?))
        } else if entry.file_type().is_symlink() {
            Entry::Link(fs::read_link(path)?)
        } else if entry.file_type().is_file() {
            let mut sha256 = Sha256::new();
            std::io::copy(&mut File::open(path)?, &mut sha256)?;
            Entry::File {
                sha256: format!("{:x}", sha256.finalize()),
                executable: mode(&entry.metadata()?),
            }
        } else {
            bail!("Unsupported entry {}.", path.display());
        };
        tree.insert(path.strip_prefix(root)?.into(), value);
    }
    Ok(tree)
}

#[cfg(unix)]
fn mode(file: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    file.permissions().mode() & 0o111
}

#[cfg(not(unix))]
fn mode(_: &fs::Metadata) -> u32 {
    0
}

pub(super) fn differences(left: &Path, right: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let left = tree(left).context("Composer vendor files")?;
    let right = tree(right).context("Phera vendor files")?;
    let paths: BTreeSet<_> = left.keys().chain(right.keys()).collect();
    let mut package = Vec::new();
    let mut generated = Vec::new();
    for path in paths {
        if left.get(path) == right.get(path) {
            continue;
        }
        let name = path.to_string_lossy().into_owned();
        if matches!(
            name.as_str(),
            "autoload.php"
                | "composer/LICENSE"
                | "composer/installed.json"
                | "composer/installed.php"
                | "composer/platform_check.php"
                | "composer/autoload_psr4.php"
                | "composer/autoload_namespaces.php"
                | "composer/autoload_classmap.php"
                | "composer/autoload_files.php"
                | "composer/autoload_static.php"
                | "composer/autoload_real.php"
        ) || path.starts_with("bin")
            || name == "composer/include_paths.php"
                && left.contains_key(path)
                && right.contains_key(path)
        {
            generated.push(name);
        } else {
            package.push(name);
        }
    }
    Ok((package, generated))
}
