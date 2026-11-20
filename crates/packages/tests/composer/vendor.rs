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

/// Keep byte comparisons separate from the semantic loader probes.
#[derive(serde::Serialize)]
pub(super) struct ByteDifference {
    path: String,
    composer_bytes: Option<usize>,
    phera_bytes: Option<usize>,
    first_byte: Option<usize>,
    cause: &'static str,
}

pub(super) fn generated_bytes(left: &Path, right: &Path) -> Result<Vec<ByteDifference>> {
    let mut names = BTreeSet::from([
        PathBuf::from("autoload.php"),
        PathBuf::from("autoload_runtime.php"),
        PathBuf::from("composer/installed.json"),
    ]);
    for vendor in [left, right] {
        for entry in fs::read_dir(vendor.join("composer"))? {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry.path().extension().is_some_and(|ext| ext == "php")
            {
                names.insert(Path::new("composer").join(entry.file_name()));
            }
        }
    }
    let mut differences = Vec::new();
    for path in names {
        let expected = bytes_if_present(&left.join(&path))?;
        let actual = bytes_if_present(&right.join(&path))?;
        if expected == actual {
            continue;
        }
        let first_byte = expected.as_ref().zip(actual.as_ref()).map(|(left, right)| {
            left.iter()
                .zip(right)
                .position(|(left, right)| left != right)
                .unwrap_or_else(|| left.len().min(right.len()))
        });
        let name = path.to_string_lossy().into_owned();
        let cause = match (expected.is_some(), actual.is_some()) {
            (true, false) => "Only Composer generated this file.",
            (false, true) => "Only Phera generated this file.",
            _ => generation_cause(&name),
        };
        differences.push(ByteDifference {
            path: name,
            composer_bytes: expected.as_ref().map(Vec::len),
            phera_bytes: actual.as_ref().map(Vec::len),
            first_byte,
            cause,
        });
    }
    Ok(differences)
}

fn bytes_if_present(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn generation_cause(path: &str) -> &'static str {
    match path {
        "autoload.php" | "composer/autoload_real.php" | "composer/autoload_static.php" => {
            "Bootstrap template, initializer suffix or path expression."
        }
        "composer/installed.json" => "JSON field order, formatting or installed metadata.",
        "composer/installed.php" => "PHP array formatting, paths or installed metadata.",
        "composer/platform_check.php" => "Generated platform guard or diagnostic formatting.",
        "composer/autoload_psr4.php"
        | "composer/autoload_namespaces.php"
        | "composer/autoload_classmap.php"
        | "composer/autoload_files.php"
        | "composer/include_paths.php" => "Map formatting, path expressions or map contents.",
        _ => "Unclassified byte difference. Inspect the generated file.",
    }
}

#[test]
fn generated_byte_report_includes_missing_files_and_first_difference() {
    let left = tempfile::tempdir().unwrap();
    let right = tempfile::tempdir().unwrap();
    for directory in [&left, &right] {
        fs::create_dir(directory.path().join("composer")).unwrap();
    }
    fs::write(left.path().join("autoload.php"), b"abc").unwrap();
    fs::write(right.path().join("autoload.php"), b"ax").unwrap();
    fs::write(right.path().join("composer/autoload_files.php"), b"extra").unwrap();
    let result = generated_bytes(left.path(), right.path()).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].first_byte, Some(1));
    assert_eq!(result[0].composer_bytes, Some(3));
    assert_eq!(result[1].composer_bytes, None);
    assert_eq!(result[1].phera_bytes, Some(5));
}
