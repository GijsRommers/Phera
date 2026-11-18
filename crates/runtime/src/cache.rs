//! Compiled code, shared by every worker thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use phera_bytecode::CompiledUnit;

use crate::RuntimeError;

/// Each PHP file is compiled once and then shared, read-only, by every
/// request on every thread. This is what makes "a fresh VM per request"
/// cheap: only the code's state is fresh, never the code.
#[derive(Clone, Default)]
pub(crate) struct CodeCache {
    /// Keyed by canonical path, so `./a.php` and `/srv/a.php` share an entry.
    units: Arc<Mutex<HashMap<PathBuf, Arc<CompiledUnit>>>>,
}

impl CodeCache {
    /// Returns the compiled unit for `path`, compiling it on first use.
    ///
    /// Reads the file and compiles, both of which block, so async callers
    /// run this on a blocking thread.
    pub(crate) fn load(&self, path: &Path) -> Result<Arc<CompiledUnit>, RuntimeError> {
        let read_error = |error| RuntimeError::Read {
            path: path.to_path_buf(),
            error,
        };
        let canonical = path.canonicalize().map_err(read_error)?;
        if let Some(unit) = self.lock().get(&canonical) {
            return Ok(unit.clone());
        }

        // Compile without holding the lock, so other files load meanwhile.
        // Two threads may compile the same file once; the result is the same.
        let source = std::fs::read(&canonical).map_err(read_error)?;
        let unit = Arc::new(phera_compiler::compile_bytes(
            &source,
            &canonical.to_string_lossy(),
        )?);
        self.lock().insert(canonical, unit.clone());
        Ok(unit)
    }

    /// A panic while the lock was held cannot leave the map half-updated
    /// (inserts are single operations), so a poisoned lock is still usable.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Arc<CompiledUnit>>> {
        self.units.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
