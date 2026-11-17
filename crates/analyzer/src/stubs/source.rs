//! Read a pinned PHP Git tree without changing its checkout: `php-8.5.12:Zend/...`.
//! This avoids silently using tomorrow's PHP signatures from a master checkout.

use std::path::Path;
use std::process::Command;

pub struct StubSource<'a> {
    root: &'a Path,
    pub commit: String,
}

impl<'a> StubSource<'a> {
    pub fn open(root: &'a Path, reference: &str) -> Result<Self, String> {
        let commit = git(root, &["rev-parse", &format!("{reference}^{{commit}}")])?;
        let result = Self {
            root,
            commit: commit.trim().into(),
        };
        let version = result.read("main/php_version.h")?;
        if !version.contains("#define PHP_MINOR_VERSION 5\n")
            || !version.contains("#define PHP_MAJOR_VERSION 8\n")
        {
            return Err(format!("{reference} is not a PHP 8.5 tree"));
        }
        Ok(result)
    }

    pub fn read(&self, path: &str) -> Result<String, String> {
        git(self.root, &["show", &format!("{}:{path}", self.commit)])
    }

    pub fn files(&self) -> Result<Vec<String>, String> {
        let paths = git(
            self.root,
            &[
                "ls-tree",
                "-r",
                "--name-only",
                &self.commit,
                "--",
                "Zend",
                "main",
                "ext",
                "sapi/cli",
            ],
        )?;
        Ok(paths
            .lines()
            .filter(|p| {
                p.ends_with(".stub.php")
                    && !p.starts_with("ext/zend_test/")
                    && !p.starts_with("ext/dl_test/")
            })
            .map(String::from)
            .collect())
    }
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
