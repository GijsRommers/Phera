//! Test support for PHP's `.phpt` test format.
//!
//! Tests come from two places under the repository's `tests/` directory:
//! upstream php-src tests listed in `tests/phpt/selected.txt` (paths relative to
//! `tests/upstream/`), and every `.phpt` file under `tests/fixtures/`.

mod expectf;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use expectf::expectf_matches;

/// A parsed `.phpt` file.
#[derive(Debug, Clone)]
pub struct Phpt {
    pub path: PathBuf,
    sections: Vec<(String, String)>,
}

impl Phpt {
    pub fn parse_file(path: &Path) -> io::Result<Self> {
        let bytes = fs::read(path)?;
        Ok(Self::parse(path, &String::from_utf8_lossy(&bytes)))
    }

    pub fn parse(path: &Path, contents: &str) -> Self {
        let mut sections: Vec<(String, String)> = Vec::new();

        for line in contents.split_inclusive('\n') {
            if let Some(name) = section_header(line) {
                sections.push((name.to_string(), String::new()));
            } else if let Some((_, body)) = sections.last_mut() {
                body.push_str(line);
            }
        }

        Self {
            path: path.to_path_buf(),
            sections,
        }
    }

    /// The test's description from its `--TEST--` section.
    pub fn title(&self) -> &str {
        self.section("TEST").map_or("", str::trim)
    }

    pub fn section(&self, name: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, body)| body.as_str())
    }

    pub fn section_names(&self) -> impl Iterator<Item = &str> {
        self.sections.iter().map(|(n, _)| n.as_str())
    }
}

/// Returns the section name if `line` is a header like `--FILE--`.
fn section_header(line: &str) -> Option<&str> {
    let name = line.trim_end().strip_prefix("--")?.strip_suffix("--")?;

    let valid = !name.is_empty() && name.chars().all(|c| c.is_ascii_uppercase() || c == '_');
    valid.then_some(name)
}

/// The repository's top-level `tests/` directory.
pub fn tests_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

/// Every test Phera should pass: the selected upstream tests, then all fixtures.
///
/// Fails if `selected.txt` names a file that doesn't exist in `tests/upstream/`.
pub fn selected_tests() -> Result<Vec<PathBuf>, String> {
    let tests = tests_dir();
    let list = tests.join("phpt/selected.txt");
    let contents =
        fs::read_to_string(&list).map_err(|e| format!("cannot read {}: {e}", list.display()))?;

    let mut paths = Vec::new();

    for (number, line) in contents.lines().enumerate() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let path = tests.join("upstream").join(line);

        if !path.is_file() {
            return Err(format!(
                "{}:{}: {line} does not exist in tests/upstream/",
                list.display(),
                number + 1
            ));
        }

        paths.push(path);
    }

    paths.extend(find_phpt_files(&tests.join("fixtures")));
    Ok(paths)
}

/// All `.phpt` files under `dir`, recursively, in sorted order.
pub fn find_phpt_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];

    while let Some(dir) = pending.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "phpt") {
                found.push(path);
            }
        }
    }

    found.sort();
    found
}

/// A path relative to the `tests/` directory, for display.
pub fn display_path(path: &Path) -> String {
    let tests = tests_dir();
    let relative = tests.canonicalize().ok().and_then(|tests| {
        path.canonicalize()
            .ok()?
            .strip_prefix(tests)
            .ok()
            .map(Path::to_path_buf)
    });

    relative
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

/// Test name filters from the command line, ignoring flags passed by cargo.
pub fn filters_from_args() -> Vec<String> {
    std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .collect()
}

/// Whether `path` matches any of `filters` (or there are no filters).
pub fn matches_filters(path: &Path, filters: &[String]) -> bool {
    let path = path.to_string_lossy();
    filters.is_empty() || filters.iter().any(|f| path.contains(f.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sections() {
        let phpt = Phpt::parse(
            Path::new("t.phpt"),
            "--TEST--\nHello\n--FILE--\n<?php\necho 1;\n?>\n--EXPECT--\n1\n",
        );

        assert_eq!(phpt.title(), "Hello");
        assert_eq!(phpt.section("FILE"), Some("<?php\necho 1;\n?>\n"));
        assert_eq!(phpt.section("EXPECT"), Some("1\n"));
        assert_eq!(phpt.section("EXPECTF"), None);
        assert_eq!(
            phpt.section_names().collect::<Vec<_>>(),
            ["TEST", "FILE", "EXPECT"]
        );
    }

    #[test]
    fn ignores_lines_that_only_look_like_headers() {
        let phpt = Phpt::parse(Path::new("t.phpt"), "--FILE--\n--x--\n-- A --\n");
        assert_eq!(phpt.section("FILE"), Some("--x--\n-- A --\n"));
    }
}
