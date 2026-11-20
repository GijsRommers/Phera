//! Compiled code, shared by every worker thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use phera_bytecode::CompiledUnit;

use crate::RuntimeError;

/// Each PHP file is compiled once and then shared, read-only, by every
/// request on every thread. This is what makes "a fresh VM per request"
/// cheap: only the code's state is fresh, never the code.
#[derive(Clone)]
pub(crate) struct CodeCache {
    /// Keyed by canonical path, so `./a.php` and `/srv/a.php` share an entry.
    units: Arc<Mutex<HashMap<PathBuf, Arc<CompiledUnit>>>>,
    registry: Arc<phera_extension_api::Registry>,
    mode: FileMode,
}

#[derive(Clone, Copy)]
pub(crate) enum FileMode {
    Web,
    Cli,
}

impl CodeCache {
    pub(crate) fn new(registry: Arc<phera_extension_api::Registry>, mode: FileMode) -> Self {
        Self {
            units: Arc::default(),
            registry,
            mode,
        }
    }

    pub(crate) async fn compile(&self, path: PathBuf) -> Result<Arc<CompiledUnit>, RuntimeError> {
        let cache = self.clone();
        tokio::task::spawn_blocking(move || cache.load(&path))
            .await
            .map_err(|error| RuntimeError::Internal(error.to_string()))?
    }

    /// Returns the compiled unit for `path`, compiling it on first use.
    ///
    /// Reads the file and compiles, both of which block, so async callers
    /// run this on a blocking thread.
    fn load(&self, path: &Path) -> Result<Arc<CompiledUnit>, RuntimeError> {
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
        let natives = phera_compiler::NativeFunctions {
            lookup: &|name| {
                let Some(entry) = self.registry.get_function(name) else {
                    return phera_vm::is_intrinsic_function(name)
                        .then_some(phera_compiler::NativeFunction::General);
                };
                Some(match entry.handler {
                    phera_extension_api::NativeFn::DeprecatedSync(_) => {
                        phera_compiler::NativeFunction::General
                    }
                    phera_extension_api::NativeFn::Async(_)
                        if self.registry.sync_selector(entry.name).is_none() =>
                    {
                        phera_compiler::NativeFunction::General
                    }
                    phera_extension_api::NativeFn::Async(_)
                        if entry
                            .params
                            .last()
                            .is_some_and(|parameter| parameter.variadic)
                            && entry.params.iter().all(|parameter| !parameter.by_ref) =>
                    {
                        phera_compiler::NativeFunction::Direct {
                            positional_by_value: 4,
                        }
                    }
                    _ => phera_compiler::NativeFunction::Direct {
                        positional_by_value: entry
                            .params
                            .iter()
                            .take_while(|parameter| !parameter.by_ref)
                            .count(),
                    },
                })
            },
            fold_string_call: &|name, argument| {
                crate::constant_calls::fold(&self.registry, name, argument)
            },
        };
        let file = canonical.to_string_lossy();
        let unit = Arc::new(match self.mode {
            FileMode::Web => phera_compiler::compile_bytes_with_natives(&source, &file, &natives),
            FileMode::Cli => {
                phera_compiler::compile_cli_bytes_with_natives(&source, &file, &natives)
            }
        }?);
        self.lock().insert(canonical, unit.clone());
        Ok(unit)
    }

    /// A panic while the lock was held cannot leave the map half-updated
    /// (inserts are single operations), so a poisoned lock is still usable.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, Arc<CompiledUnit>>> {
        self.units.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// An include uses the same native inventory as the file that includes it.
impl phera_vm::CodeLoader for CodeCache {
    fn load(
        &self,
        path: PathBuf,
    ) -> phera_extension_api::LocalFuture<'_, Result<Arc<CompiledUnit>, phera_vm::CodeLoadError>>
    {
        Box::pin(async move {
            self.compile(path).await.map_err(|error| match error {
                RuntimeError::Compile(error) => phera_vm::CodeLoadError::Compile(error),
                error => phera_vm::CodeLoadError::Unavailable(error.to_string()),
            })
        })
    }
}
