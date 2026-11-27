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
    /// PHP that Phera ships (an adapter's entry script, a bundled class), by
    /// the name it is compiled under. It has no file to check for changes.
    shipped: Arc<Mutex<HashMap<String, Arc<CompiledUnit>>>>,
    /// Optional pre-compiled `.phb` AOT image backing this cache.
    image: Option<Arc<phera_aot::AotImage>>,
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

struct ImageUnitMatch<'a> {
    image: Arc<phera_aot::AotImage>,
    entry: &'a phera_aot::ImageFileEntry,
    unit_id: u32,
}

impl CodeCache {
    pub(crate) fn new(registry: Arc<phera_extension_api::Registry>, mode: FileMode) -> Self {
        Self {
            units: Arc::default(),
            registry,
            mode,
            shipped: Arc::default(),
            image: None,
        }
    }

    pub(crate) fn with_image(mut self, image: Arc<phera_aot::AotImage>) -> Self {
        self.image = Some(image);
        self
    }

    fn image_target(&self) -> phera_aot::ImageTarget {
        match self.mode {
            FileMode::Web => phera_aot::ImageTarget::Web,
            FileMode::Cli => phera_aot::ImageTarget::Cli,
        }
    }

    fn image_unit_for(
        &self,
        path: &Path,
        canonical: Option<&Path>,
    ) -> Result<Option<ImageUnitMatch<'_>>, RuntimeError> {
        let Some(image) = &self.image else {
            return Ok(None);
        };
        let target = self.image_target();
        let resolved_abs = if canonical.is_none() && path.is_relative() {
            std::env::current_dir().ok().map(|cwd| cwd.join(path))
        } else {
            None
        };
        let entry = canonical
            .and_then(|c| image.find_canonical(c))
            .or_else(|| {
                resolved_abs
                    .as_deref()
                    .and_then(|p| image.find_canonical(p))
            })
            .or_else(|| image.find_canonical(path));
        let Some(entry) = entry else {
            if image.mode() == phera_bytecode::facts::BuildMode::Strict {
                return Err(RuntimeError::Read {
                    path: path.to_path_buf(),
                    error: std::io::ErrorKind::NotFound.into(),
                });
            }
            return Ok(None);
        };
        let unit_id = match target {
            phera_aot::ImageTarget::Web => entry.unit_web,
            phera_aot::ImageTarget::Cli => entry.unit_cli,
        };
        let Some(unit_id) = unit_id else {
            if image.mode() == phera_bytecode::facts::BuildMode::Strict {
                let target_name = match target {
                    phera_aot::ImageTarget::Web => "web",
                    phera_aot::ImageTarget::Cli => "cli",
                };
                return Err(RuntimeError::Internal(format!(
                    "Phera AOT image has no {target_name} units; rebuild with `phera build --target {target_name}` or `--target both`"
                )));
            }
            return Ok(None);
        };
        Ok(Some(ImageUnitMatch {
            image: image.clone(),
            entry,
            unit_id,
        }))
    }

    fn load_image_unit(
        image: &phera_aot::AotImage,
        unit_id: u32,
    ) -> Result<Arc<CompiledUnit>, RuntimeError> {
        image
            .load_unit(unit_id)
            .map_err(|error| RuntimeError::Internal(error.to_string()))
    }

    pub(crate) async fn compile(&self, path: PathBuf) -> Result<Arc<CompiledUnit>, RuntimeError> {
        let validation = match self.mode {
            FileMode::Cli => Validation::Frozen,
            FileMode::Web => Validation::Timestamps,
        };
        self.compile_candidates(vec![path], validation).await
    }

    /// The blocking CLI can read its entry file itself. Compilation still
    /// uses the compiler's dedicated stack.
    pub(crate) fn compile_cli_entry(&self, path: &Path) -> Result<Arc<CompiledUnit>, RuntimeError> {
        self.load(path, Validation::Frozen)
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
        if self.image.is_some() {
            let canonical = path.canonicalize().ok();
            let found = self.image_unit_for(path, canonical.as_deref())?;
            if let Some(matched) = found {
                // Strict mode or stripped source on disk: serve straight from
                // the image with zero disk reads.
                if matched.image.mode() == phera_bytecode::facts::BuildMode::Strict
                    || canonical.is_none()
                {
                    return Self::load_image_unit(&matched.image, matched.unit_id);
                }
                let canonical = canonical.expect("checked Some above");
                return self.load_hybrid_cached(
                    &canonical,
                    path,
                    validation,
                    &matched.image,
                    matched.entry.source_len,
                    matched.entry.source_hash,
                    matched.unit_id,
                );
            }
            let canonical = canonical.ok_or_else(|| RuntimeError::Read {
                path: path.to_path_buf(),
                error: std::io::ErrorKind::NotFound.into(),
            })?;
            return self.load_canonical(&canonical, path, validation, CompileStack::Dedicated);
        }
        let canonical = path.canonicalize().map_err(|error| RuntimeError::Read {
            path: path.to_path_buf(),
            error,
        })?;
        self.load_canonical(&canonical, path, validation, CompileStack::Dedicated)
    }

    /// Hybrid mode: caches `FileStamp -> Unit` so unchanged files serve the
    /// pre-compiled image unit without re-hashing on every revalidation, while
    /// edited files transparently recompile from disk (including on first load
    /// under `Validation::Frozen`, such as CLI runs).
    #[allow(clippy::too_many_arguments)]
    fn load_hybrid_cached(
        &self,
        canonical: &Path,
        path: &Path,
        validation: Validation,
        image: &phera_aot::AotImage,
        expected_len: u64,
        expected_hash: [u8; 16],
        unit_id: u32,
    ) -> Result<Arc<CompiledUnit>, RuntimeError> {
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
        let Ok(stamp) = FileStamp::read(canonical) else {
            return Self::load_image_unit(image, unit_id);
        };
        if let Some(cached) = revision.as_ref()
            && cached.stamp == stamp
        {
            return cached.compiled.clone().map_err(RuntimeError::Compile);
        }
        let source = std::fs::read(canonical).map_err(|error| RuntimeError::Read {
            path: path.to_path_buf(),
            error,
        })?;
        if source.len() as u64 == expected_len && phera_aot::blake2b_128(&source) == expected_hash {
            let unit = Self::load_image_unit(image, unit_id)?;
            *revision = Some(CachedRevision {
                stamp,
                compiled: Ok(unit.clone()),
            });
            return Ok(unit);
        }
        let compiled = self.compile_source(
            &source,
            &canonical.to_string_lossy(),
            CompileStack::Dedicated,
        );
        *revision = Some(CachedRevision {
            stamp,
            compiled: compiled.clone(),
        });
        compiled.map_err(RuntimeError::Compile)
    }

    /// The preload pool provides a reusable large stack for each blocking thread.
    pub(crate) fn preload_file(&self, path: &Path) -> Result<Arc<CompiledUnit>, RuntimeError> {
        if self.image.is_some() {
            return self.load(path, Validation::Timestamps);
        }
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

    /// Compiles PHP source that Phera ships under the name `file`, once (see
    /// `crate::adapters`). The name may be the path of a real file, such as
    /// the Laravel entry's `public/index.php`; the file is not read.
    pub(crate) fn compile_shipped(
        &self,
        source: &str,
        file: &str,
    ) -> Result<Arc<CompiledUnit>, RuntimeError> {
        let mut shipped = self.shipped.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(unit) = shipped.get(file) {
            return Ok(unit.clone());
        }
        let unit = self
            .compile_source(source.as_bytes(), file, CompileStack::Dedicated)
            .map_err(RuntimeError::Compile)?;
        shipped.insert(file.to_owned(), unit.clone());
        Ok(unit)
    }

    fn compile_source(
        &self,
        source: &[u8],
        file: &str,
        stack: CompileStack,
    ) -> Result<Arc<CompiledUnit>, CompileError> {
        compile_unit_on_stack(&self.registry, self.mode, source, file, stack).map(|mut unit| {
            // Every file from the cache gets its process number and name keys,
            // on the command line too: its classes are then found by key, as
            // a web request finds them.
            unit.metadata = Some(Arc::new(phera_bytecode::UnitMetadata::new(&unit)));
            Arc::new(unit)
        })
    }
}

pub(crate) fn compile_unit_with_registry(
    registry: &phera_extension_api::Registry,
    mode: FileMode,
    source: &[u8],
    file: &str,
) -> Result<CompiledUnit, CompileError> {
    compile_unit_on_stack(registry, mode, source, file, CompileStack::Dedicated)
}

fn compile_unit_on_stack(
    registry: &phera_extension_api::Registry,
    mode: FileMode,
    source: &[u8],
    file: &str,
    stack: CompileStack,
) -> Result<CompiledUnit, CompileError> {
    let natives = phera_compiler::NativeFunctions {
        lookup: &|name| {
            let Some(entry) = registry.get_function(name) else {
                return phera_vm::is_intrinsic_function(name)
                    .then_some(phera_compiler::NativeFunction::General);
            };
            Some(match entry.handler {
                phera_extension_api::NativeFn::DeprecatedSync(_)
                | phera_extension_api::NativeFn::DeprecatedAsync(_) => {
                    phera_compiler::NativeFunction::General
                }
                phera_extension_api::NativeFn::Async(_)
                    if registry.sync_selector(entry.name).is_none() =>
                {
                    phera_compiler::NativeFunction::General
                }
                phera_extension_api::NativeFn::Async(_)
                | phera_extension_api::NativeFn::Sync(_)
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
        fold_string_call: &|name, argument| crate::constant_calls::fold(registry, name, argument),
    };
    match (mode, stack) {
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
        let strict = self
            .image
            .as_deref()
            .is_some_and(|img| img.mode() == phera_bytecode::facts::BuildMode::Strict);
        Box::pin(async move {
            if strict {
                return Err(phera_extension_api::NativeError::throw(
                    "Phera\\SealedImageException",
                    "stream includes are disabled in strict AOT image mode",
                ));
            }
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

    fn load_bundled(
        &self,
        source: phera_extension_api::BundledSource,
    ) -> phera_extension_api::LocalFuture<'_, Result<Arc<CompiledUnit>, phera_vm::CodeLoadError>>
    {
        Box::pin(async move {
            self.compile_shipped(source.source, source.file)
                .map_err(include_error)
        })
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

    fn build_single_file_image(
        root: &Path,
        file: &Path,
        source: &[u8],
        mode: phera_bytecode::facts::BuildMode,
    ) -> Arc<phera_aot::AotImage> {
        let canonical = file.canonicalize().unwrap();
        let canonical_root = root.canonicalize().unwrap();
        let rel = canonical
            .strip_prefix(&canonical_root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let unit =
            Arc::new(phera_compiler::compile_bytes(source, &canonical.to_string_lossy()).unwrap());
        let abi = phera_aot::runtime_abi_hash([0u8; 16]);
        let bytes = phera_aot::encode_image(&phera_aot::ImageInput {
            runtime_abi_hash: abi,
            manifest: phera_aot::ImageManifest {
                project_root: canonical_root.to_string_lossy().into_owned(),
                mode,
                has_web: true,
                has_cli: true,
                allow: Vec::new(),
                framework: None,
                entries: vec![rel.clone()],
                include_globs: Vec::new(),
                exclude_globs: Vec::new(),
                include_dev: false,
            },
            files: vec![phera_aot::ImageFileEntry {
                relative_path: rel,
                canonical_path: canonical.to_string_lossy().into_owned(),
                unit_web: Some(0),
                unit_cli: Some(0),
                source_len: source.len() as u64,
                source_hash: phera_aot::blake2b_128(source),
                flags: phera_aot::FileFlags::ENTRY,
            }],
            classmap: Vec::new(),
            units: vec![unit],
            program_facts: None,
            audit: phera_bytecode::facts::DynamicAudit::default(),
        })
        .unwrap();
        phera_aot::AotImage::from_bytes(bytes.into(), Some(abi)).unwrap()
    }

    #[tokio::test]
    async fn hybrid_image_serves_cached_unit_and_recompiles_on_disk_edit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("app.php");
        let original = b"<?php echo 'from-image';";
        std::fs::write(&file, original).unwrap();

        let image = build_single_file_image(
            dir.path(),
            &file,
            original,
            phera_bytecode::facts::BuildMode::Hybrid,
        );
        let cache = CodeCache::new(
            Arc::new(phera_extension_api::Registry::new()),
            FileMode::Web,
        )
        .with_image(image.clone());

        let loaded = cache.compile(file.clone()).await.unwrap();
        let expected = image.load_unit(0).unwrap();
        assert!(Arc::ptr_eq(&loaded, &expected));

        // Modifying the file on disk causes hybrid mode to recompile from disk.
        std::fs::write(&file, b"<?php echo 'from-disk-edited';").unwrap();
        let reloaded = cache.compile(file.clone()).await.unwrap();
        assert!(!Arc::ptr_eq(&reloaded, &expected));
    }

    #[tokio::test]
    async fn strict_image_serves_deleted_source_and_rejects_unknown_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("app.php");
        let other = dir.path().join("extra.php");
        let original = b"<?php echo 'sealed';";
        std::fs::write(&file, original).unwrap();
        std::fs::write(&other, b"<?php echo 'outside';").unwrap();

        let canonical_file = file.canonicalize().unwrap();
        let image = build_single_file_image(
            dir.path(),
            &file,
            original,
            phera_bytecode::facts::BuildMode::Strict,
        );
        // Remove the original PHP source file on disk!
        std::fs::remove_file(&file).unwrap();

        let cache = CodeCache::new(
            Arc::new(phera_extension_api::Registry::new()),
            FileMode::Web,
        )
        .with_image(image.clone());

        // Still loads cleanly from the strict image even with source deleted.
        let loaded = cache.compile(canonical_file).await.unwrap();
        assert!(Arc::ptr_eq(&loaded, &image.load_unit(0).unwrap()));

        // A file outside the image is refused even though it exists on disk.
        assert!(matches!(
            cache.compile(other).await,
            Err(RuntimeError::Read { .. })
        ));
    }
}
