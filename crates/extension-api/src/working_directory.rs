//! A request-local working directory for file operations and PHP includes.

use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct WorkingDirectory(Option<PathBuf>);

impl Default for WorkingDirectory {
    fn default() -> Self {
        Self(std::env::current_dir().ok())
    }
}

impl WorkingDirectory {
    pub fn path(&self) -> Option<&Path> {
        self.0.as_deref()
    }

    pub fn set(&mut self, path: PathBuf) {
        self.0 = Some(path);
    }

    pub fn resolve(&self, name: &Path) -> io::Result<PathBuf> {
        if name.is_absolute() {
            return Ok(name.to_owned());
        }
        let cwd = self.path().ok_or(io::ErrorKind::NotFound)?;
        Ok(cwd.join(name))
    }
}
