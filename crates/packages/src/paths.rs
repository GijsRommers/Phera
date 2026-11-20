//! Confine package writes to an owned directory, e.g. `vendor/acme/library`.
//!
//! Existing symlink ancestors are rejected before writing untrusted package paths.

use anyhow::{Result, bail};
use std::path::{Component, Path, PathBuf};

pub(crate) fn relative(path: &str) -> Result<PathBuf> {
    if path.contains('\\') || path.contains('\0') || path.contains(':') {
        bail!("Unsafe package path {path:?}.");
    }
    let path = Path::new(path);
    if path
        .components()
        .any(|part| !matches!(part, Component::Normal(_) | Component::CurDir))
    {
        bail!(
            "Package path must stay inside its directory: {}",
            path.display()
        );
    }
    Ok(path.to_path_buf())
}

pub(crate) fn inside(root: &Path, name: &str) -> Result<PathBuf> {
    let mut current = root.to_path_buf();
    for part in relative(name)?.components() {
        current.push(part);
        if current
            .symlink_metadata()
            .is_ok_and(|metadata| metadata.is_symlink())
        {
            bail!("Refusing to write through symlink {}.", current.display());
        }
    }
    Ok(current)
}
