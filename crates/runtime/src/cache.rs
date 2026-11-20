//! `require 'vendor/autoload.php';` runs in each PHP request, but workers
//! only compile one copy of that file revision, even on simultaneous misses.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use phera_bytecode::CompiledUnit;
use phera_compiler::CompileError;

use crate::RuntimeError;

/// Each PHP file is compiled once and then shared, read-only, by every
/// request on every thread. This is what makes "a fresh VM per request"
/// cheap: only the code's state is fresh, never the code.
#[derive(Clone)]
pub(crate) struct CodeCache {
    /// Keyed by canonical path, so `./a.php` and `/srv/a.php` share an entry.
    units: Arc<Mutex<HashMap<PathBuf, FileSlot>>>,
    registry: Arc<phera_extension_api::Registry>,
    mode: FileMode,
}

type FileSlot = Arc<Mutex<Option<CachedRevision>>>;

struct CachedRevision {
    stamp: FileStamp,
    compiled: Result<Arc<CompiledUnit>, CompileError>,
}

#[derive(PartialEq, Eq)]
struct FileStamp {
    modified: SystemTime,
    len: u64,
    #[cfg(unix)]
    inode: (u64, u64),
}

impl FileStamp {
    fn read(path: &Path) -> std::io::Result<Self> {
        let metadata = path.metadata()?;
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            modified: metadata.modified()?,
            len: metadata.len(),
            #[cfg(unix)]
            inode: (metadata.dev(), metadata.ino()),
        })
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Validation {
    Frozen,
    Timestamps,
}

#[derive(Clone, Copy)]
pub(crate) enum FileMode {
    Web,
    Cli,
}

#[derive(Clone, Copy)]
enum CompileStack {
    Dedicated,
    PreloadWorker,
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
        let validation = match self.mode {
            FileMode::Cli => Validation::Frozen,
            FileMode::Web => Validation::Timestamps,
        };
        self.compile_candidates(vec![path], validation).await
    }

    pub(crate) async fn compile_candidates(
        &self,
        candidates: Vec<PathBuf>,
        validation: Validation,
    ) -> Result<Arc<CompiledUnit>, RuntimeError> {
        let cache = self.clone();
        tokio::task::spawn_blocking(move || {
            let mut last = RuntimeError::Read {
                path: PathBuf::new(),
                error: std::io::ErrorKind::NotFound.into(),
            };
            for candidate in candidates {
                match cache.load(&candidate, validation) {
                    Err(error @ RuntimeError::Read { .. }) => last = error,
                    result => return result,
                }
            }
            Err(last)
        })
        .await
        .map_err(|error| RuntimeError::Internal(error.to_string()))?
    }

    /// Only requests for the same canonical path wait for one another.
    fn load(&self, path: &Path, validation: Validation) -> Result<Arc<CompiledUnit>, RuntimeError> {
        let canonical = path.canonicalize().map_err(|error| RuntimeError::Read {
            path: path.to_path_buf(),
            error,
        })?;
        self.load_canonical(&canonical, path, validation, CompileStack::Dedicated)
    }

    /// The preload pool provides a reusable large stack for each blocking thread.
    pub(crate) fn preload_file(&self, path: &Path) -> Result<Arc<CompiledUnit>, RuntimeError> {
        self.load_canonical(
            path,
            path,
            Validation::Timestamps,
            CompileStack::PreloadWorker,
        )
    }

    fn load_canonical(
        &self,
        canonical: &Path,
        path: &Path,
        validation: Validation,
        stack: CompileStack,
    ) -> Result<Arc<CompiledUnit>, RuntimeError> {
        let read_error = |error| RuntimeError::Read {
            path: path.to_path_buf(),
            error,
        };
        let slot = self
            .units
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(canonical.to_path_buf())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone();
        let mut revision = slot.lock().unwrap_or_else(PoisonError::into_inner);
        if matches!(validation, Validation::Frozen)
            && let Some(cached) = revision.as_ref()
            && cached.compiled.is_ok()
        {
            return cached.compiled.clone().map_err(RuntimeError::Compile);
        }
        let stamp = FileStamp::read(canonical).map_err(read_error)?;
        if let Some(cached) = revision.as_ref()
            && cached.stamp == stamp
        {
            return cached.compiled.clone().map_err(RuntimeError::Compile);
        }
        let source = std::fs::read(canonical).map_err(read_error)?;
        let compiled = self.compile_source(&source, &canonical.to_string_lossy(), stack);
        *revision = Some(CachedRevision {
            stamp,
            compiled: compiled.clone(),
        });
        compiled.map_err(RuntimeError::Compile)
    }

    fn compile_source(
        &self,
        source: &[u8],
        file: &str,
        stack: CompileStack,
    ) -> Result<Arc<CompiledUnit>, CompileError> {
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
        match (self.mode, stack) {
            (FileMode::Web, CompileStack::PreloadWorker) => {
                phera_compiler::compile_bytes_with_natives_on_stack(source, file, &natives)
            }
            (FileMode::Web, CompileStack::Dedicated) => {
                phera_compiler::compile_bytes_with_natives(source, file, &natives)
            }
            (FileMode::Cli, _) => {
                phera_compiler::compile_cli_bytes_with_natives(source, file, &natives)
            }
        }
        .map(|mut unit| {
            if matches!(self.mode, FileMode::Web) {
                unit.metadata = Some(Arc::new(phera_bytecode::UnitMetadata::new(&unit)));
            }
            Arc::new(unit)
        })
    }
}

/// An include uses the same native inventory as the file that includes it.
impl phera_vm::CodeLoader for CodeCache {
    fn wraps_include(&self, context: &mut dyn phera_extension_api::Context, path: &[u8]) -> bool {
        phera_ext_streams::wraps_include(context, path)
    }

    fn load_stream<'a>(
        &'a self,
        context: &'a mut dyn phera_extension_api::Context,
        path: &'a [u8],
        operation: &'a str,
        once: bool,
    ) -> phera_extension_api::LocalFuture<
        'a,
        Result<phera_vm::StreamInclude, phera_extension_api::NativeError>,
    > {
        use phera_ext_streams::IncludeSource;
        use phera_vm::StreamInclude;
        Box::pin(async move {
            let source = phera_ext_streams::include_source(context, path, operation, once).await?;
            Ok(match source {
                None => StreamInclude::LocalFile,
                Some(IncludeSource::Unavailable) => StreamInclude::Unavailable,
                Some(IncludeSource::AlreadyIncluded) => StreamInclude::AlreadyIncluded,
                Some(IncludeSource::Source { name, bytes }) => StreamInclude::Compiled(
                    self.compile_source(&bytes, &name, CompileStack::Dedicated),
                ),
            })
        })
    }

    fn load(
        &self,
        path: PathBuf,
    ) -> phera_extension_api::LocalFuture<'_, Result<Arc<CompiledUnit>, phera_vm::CodeLoadError>>
    {
        Box::pin(async move { self.compile(path).await.map_err(include_error) })
    }
}

pub(crate) fn include_error(error: RuntimeError) -> phera_vm::CodeLoadError {
    match error {
        RuntimeError::Compile(error) => phera_vm::CodeLoadError::Compile(error),
        error => phera_vm::CodeLoadError::Unavailable(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn concurrent_misses_share_the_same_compilation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("shared.php");
        let alternate = directory.path().join("./shared.php");
        let source = (0..500)
            .map(|i| format!("function f{i}() {{}}\n"))
            .collect::<String>();
        std::fs::write(&path, format!("<?php\n{source}")).unwrap();
        let cache = CodeCache::new(
            Arc::new(phera_extension_api::Registry::new()),
            FileMode::Web,
        );
        let mut jobs = tokio::task::JoinSet::new();
        for index in 0..12 {
            let cache = cache.clone();
            let path = if index % 2 == 0 { &path } else { &alternate }.clone();
            jobs.spawn(async move { cache.compile(path).await.unwrap() });
        }
        let first = jobs.join_next().await.unwrap().unwrap();
        while let Some(unit) = jobs.join_next().await {
            assert!(Arc::ptr_eq(&first, &unit.unwrap()));
        }
        std::fs::write(&path, "<?php echo 'edited';").unwrap();
        let edited = cache.compile(path).await.unwrap();
        assert!(!Arc::ptr_eq(&first, &edited));
    }
}
