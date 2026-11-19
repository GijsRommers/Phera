//! Composer project settings. With `"App\\": "src/"`, `src/Model/User.php`
//! has namespace `App\Model`.

use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub(crate) struct Manifest {
    root: PathBuf,
    data: Value,
}

impl Manifest {
    pub(crate) fn discover(paths: &[String]) -> Option<Self> {
        let current = std::env::current_dir().ok()?;
        let root = paths
            .iter()
            .map(|path| current.join(path))
            .find_map(|path| project_root(&path))
            .or_else(|| project_root(&current))?;
        let source = std::fs::read(root.join("composer.json")).ok()?;
        let data = serde_json::from_slice(&source).ok()?;
        Some(Self { root, data })
    }

    pub(crate) fn namespace_matches(&self, file: &Path, actual: &str) -> Option<bool> {
        let parent = file.canonicalize().ok()?;
        let parent = parent.parent()?;
        let mut matched = false;
        for section in ["autoload", "autoload-dev"] {
            let Some(mappings) = self
                .data
                .get(section)
                .and_then(|list| list.get("psr-4"))
                .and_then(Value::as_object)
            else {
                continue;
            };
            for (prefix, paths) in mappings {
                let paths = paths
                    .as_array()
                    .map_or_else(|| std::slice::from_ref(paths), Vec::as_slice);
                for directory in paths.iter().filter_map(Value::as_str) {
                    let root = self.root.join(directory).canonicalize().ok();
                    let Some(relative) = root
                        .as_ref()
                        .and_then(|root| parent.strip_prefix(root).ok())
                    else {
                        continue;
                    };
                    matched = true;
                    let mut namespace = prefix.trim_end_matches('\\').to_owned();
                    for component in relative.components() {
                        if !namespace.is_empty() {
                            namespace.push('\\');
                        }
                        namespace.push_str(&component.as_os_str().to_string_lossy());
                    }
                    if namespace == actual {
                        return Some(true);
                    }
                }
            }
        }
        matched.then_some(false)
    }
}

fn project_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|path| path.join("composer.json").is_file())
        .map(Path::to_path_buf)
}
