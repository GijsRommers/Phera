//! What a project sets for `phera serve` in its `phera.toml`: the
//! `--preload` option, and the warm-up requests of `--warmup`.
//!
//! ```toml
//! [server]
//! preload = true
//! warmup = ["/api/posts?page=1", "/"]
//! ```

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct Settings {
    pub(super) preload: bool,
    /// The paths handled as requests before the snapshot (see `--warmup`).
    pub(super) warmup: Vec<String>,
    pub(super) path: Option<PathBuf>,
}

pub(super) fn project_root(document_root: &Path) -> &Path {
    let is_project = |path: &Path| {
        path.join("composer.json").is_file()
            || path.join("phera.toml").is_file()
            || path.join("vendor/composer").is_dir()
    };
    if is_project(document_root) {
        document_root
    } else {
        document_root
            .parent()
            .filter(|path| is_project(path))
            .unwrap_or(document_root)
    }
}

pub(super) fn read(project: &Path) -> Result<Settings, String> {
    let path = project.join("phera.toml");
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Settings::default()),
        Err(error) => return Err(format!("{}: {error}", path.display())),
    };
    let table: toml::Table = source
        .parse()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let preload = table.get("server").and_then(|server| server.get("preload"));
    let preload = match preload {
        Some(toml::Value::Boolean(enabled)) => *enabled,
        Some(_) => {
            return Err(format!(
                "{}: server.preload must be true or false",
                path.display()
            ));
        }
        None => false,
    };
    let warmup = match table.get("server").and_then(|server| server.get("warmup")) {
        Some(toml::Value::Array(paths)) => paths
            .iter()
            .map(|entry| match entry {
                toml::Value::String(entry) if crate::arguments::is_warmup_path(entry) => {
                    Ok(entry.clone())
                }
                _ => Err(format!(
                    "{}: server.warmup must be a list of paths like \"/api/posts?page=1\", without spaces",
                    path.display()
                )),
            })
            .collect::<Result<Vec<String>, String>>()?,
        Some(_) => {
            return Err(format!(
                "{}: server.warmup must be a list of paths",
                path.display()
            ));
        }
        None => Vec::new(),
    };
    Ok(Settings {
        preload,
        warmup,
        path: path.canonicalize().ok(),
    })
}
