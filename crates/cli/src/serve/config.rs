//! The project can turn `phera serve --preload` on in `phera.toml`.
//!
//! ```toml
//! [server]
//! preload = true
//! ```

use std::path::{Path, PathBuf};

#[derive(Default)]
pub(super) struct Settings {
    pub(super) preload: bool,
    pub(super) path: Option<PathBuf>,
    pub(super) table: toml::Table,
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
    let (table, config_path) = crate::debug_settings::read_table(project)?;
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
    Ok(Settings {
        preload,
        path: config_path,
        table,
    })
}
