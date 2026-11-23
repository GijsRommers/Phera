//! Composer resolution: "Vendor\\Package\\Client" can live in vendor/pkg/src/Client.php.
//! Generated maps describe where to look; dependencies are read only as needed.

pub(crate) mod manifest;
mod metadata;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Autoload {
    pub vendor: PathBuf,
    pub files: Vec<PathBuf>,
    pub classmap: BTreeMap<String, PathBuf>,
    pub prefixes: BTreeMap<String, Vec<PathBuf>>,
}

impl Autoload {
    pub fn discover(paths: &[String]) -> Result<Option<Self>, String> {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = paths
            .iter()
            .filter_map(|path| Path::new(path).canonicalize().ok())
            .find_map(|path| project_root(&path))
            .or_else(|| project_root(&cwd));
        root.map(|root| metadata::load(&root)).transpose()
    }

    pub fn class_file(&self, name: &str, key: &str) -> Option<PathBuf> {
        if let Some(path) = self.classmap.get(key) {
            return Some(path.clone());
        }
        if name.contains(['/', '.']) {
            return None;
        }
        // Composer tries the longest matching prefix first, then shorter ones.
        name.match_indices('\\')
            .map(|(offset, _)| offset + 1)
            .rev()
            .chain(std::iter::once(0))
            .find_map(|length| {
                let directories = self.prefixes.get(&name[..length])?;
                let relative = format!("{}.php", name[length..].replace('\\', "/"));
                directories
                    .iter()
                    .map(|directory| directory.join(&relative))
                    .find(|path| path.is_file())
            })
    }
}

fn project_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|p| p.join("composer.json").is_file() && p.join("vendor/composer").is_dir())
        .map(Path::to_path_buf)
}
