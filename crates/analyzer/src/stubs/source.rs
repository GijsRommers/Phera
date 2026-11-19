//! Read a pinned PHP Git tree without changing its checkout: `php-8.5.12:Zend/...`.
//! This avoids silently using tomorrow's PHP signatures from a master checkout.
//! The selected tree supplies PHP declarations such as `class DateTime { ... }`.

use phera_source::LanguageLevel;
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct StubSource<'a> {
    root: &'a Path,
    pub provenance: String,
    pub level: LanguageLevel,
    commit: Option<String>,
}

impl<'a> StubSource<'a> {
    pub fn open(root: &'a Path, reference: &str) -> Result<Self, String> {
        let commit = if reference == "--release-source" {
            None
        } else {
            Some(
                git(root, &["rev-parse", &format!("{reference}^{{commit}}")])?
                    .trim()
                    .to_owned(),
            )
        };
        let mut result = Self {
            root,
            provenance: String::new(),
            level: LanguageLevel::default(),
            commit,
        };
        let header = result.read("main/php_version.h")?;
        let define = |name: &str| {
            header.lines().find_map(|line| {
                line.strip_prefix("#define ")?
                    .strip_prefix(name)?
                    .strip_prefix(' ')
            })
        };
        let major = define("PHP_MAJOR_VERSION").ok_or("Missing PHP_MAJOR_VERSION")?;
        let minor = define("PHP_MINOR_VERSION").ok_or("Missing PHP_MINOR_VERSION")?;
        result.level = format!("{major}.{minor}").parse()?;
        result.provenance = result.commit.clone().unwrap_or_else(|| {
            format!(
                "release source {}",
                define("PHP_VERSION").unwrap_or_default().trim_matches('"')
            )
        });
        Ok(result)
    }

    pub fn read(&self, path: &str) -> Result<String, String> {
        if let Some(commit) = &self.commit {
            return git(self.root, &["show", &format!("{commit}:{path}")]);
        }
        let path = self.root.join(path);
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))
    }

    pub fn files(&self) -> Result<Vec<String>, String> {
        let Some(commit) = &self.commit else {
            return release_files(self.root);
        };
        let paths = git(
            self.root,
            &[
                "ls-tree",
                "-r",
                "--name-only",
                commit,
                "--",
                "Zend",
                "main",
                "ext",
                "sapi/cli",
            ],
        )?;
        Ok(paths
            .lines()
            .filter(|path| is_stub(path))
            .map(String::from)
            .collect())
    }
}

fn is_stub(path: &str) -> bool {
    path.ends_with(".stub.php")
        && !path.starts_with("ext/zend_test/")
        && !path.starts_with("ext/dl_test/")
}

fn release_files(root: &Path) -> Result<Vec<String>, String> {
    let mut pending = ["Zend", "main", "ext", "sapi/cli"]
        .map(PathBuf::from)
        .to_vec();
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries =
            std::fs::read_dir(root.join(&directory)).map_err(|error| error.to_string())?;
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = directory.join(entry.file_name());
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file()
                && let Some(path) = path.to_str()
                && is_stub(path)
            {
                files.push(path.to_owned());
            }
        }
    }
    files.sort();
    Ok(files)
}

fn git(root: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into());
    }
    String::from_utf8(output.stdout).map_err(|e| e.to_string())
}
