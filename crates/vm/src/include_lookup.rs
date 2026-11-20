//! `include 'helper.php'` searches include_path and the caller's directory,
//! while `include './helper.php'` searches only the current working directory.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use phera_bytecode::CompiledUnit;
use phera_extension_api::ini::{boolean, integer};
use phera_extension_api::{Context, LocalFuture, NativeError};

/// The file spelling and the request state that affect its resolution.
pub enum IncludeLookup<'a> {
    Absolute(&'a OsStr),
    Relative(RelativeInclude),
}

/// The same relative spelling can select a different file after chdir() or
/// set_include_path(), or when it appears in another directory.
#[derive(Hash, PartialEq, Eq)]
pub struct RelativeInclude {
    pub(crate) name: OsString,
    pub(crate) cwd: IncludeDirectory,
    pub(crate) caller: Option<PathBuf>,
    pub(crate) include_path: Option<OsString>,
}

#[derive(Hash, PartialEq, Eq)]
pub(crate) enum IncludeDirectory {
    Process,
    Known(PathBuf),
    Missing,
}

impl IncludeLookup<'_> {
    pub fn candidates(&self) -> Vec<PathBuf> {
        match self {
            Self::Absolute(path) => vec![PathBuf::from(path)],
            Self::Relative(relative) => relative.candidates(),
        }
    }
}

impl RelativeInclude {
    fn candidates(&self) -> Vec<PathBuf> {
        let path = Path::new(&self.name);
        let Some(include_path) = &self.include_path else {
            return self.resolve(path.to_path_buf()).into_iter().collect();
        };
        let mut candidates = Vec::new();
        for directory in std::env::split_paths(include_path) {
            let candidate = if directory == Path::new(".") {
                path.to_path_buf()
            } else {
                directory.join(path)
            };
            candidates.push(candidate);
        }
        if let Some(parent) = &self.caller {
            let candidate = parent.join(path);
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
        if !candidates.iter().any(|candidate| candidate == path) {
            candidates.push(path.to_path_buf());
        }
        candidates
            .into_iter()
            .filter_map(|path| self.resolve(path))
            .collect()
    }

    fn resolve(&self, path: PathBuf) -> Option<PathBuf> {
        if path.is_absolute() {
            return Some(path);
        }
        match &self.cwd {
            IncludeDirectory::Process => Some(path),
            IncludeDirectory::Known(cwd) => Some(cwd.join(path)),
            IncludeDirectory::Missing => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_working_directory_still_allows_absolute_candidates() {
        let root = std::env::current_dir().unwrap();
        let caller = root.join("caller");
        let include = root.join("include");
        let lookup = RelativeInclude {
            name: "helper.php".into(),
            cwd: IncludeDirectory::Missing,
            caller: Some(caller.clone()),
            include_path: Some(std::env::join_paths([Path::new("."), &include]).unwrap()),
        };
        assert_eq!(
            lookup.candidates(),
            [include.join("helper.php"), caller.join("helper.php")]
        );
    }
}

/// PHP can serve a cached include until its next timestamp or path check.
#[derive(Clone, Copy)]
pub struct IncludeCachePolicy {
    pub validate_timestamps: bool,
    pub revalidate_freq: Duration,
    pub revalidate_path: bool,
}

impl Default for IncludeCachePolicy {
    fn default() -> Self {
        Self {
            validate_timestamps: true,
            revalidate_freq: Duration::from_secs(2),
            revalidate_path: false,
        }
    }
}

impl IncludeCachePolicy {
    pub fn set_ini(&mut self, name: &str, value: &[u8]) {
        match name {
            "opcache.validate_timestamps" => self.validate_timestamps = boolean(value),
            "opcache.revalidate_freq" => {
                self.revalidate_freq = Duration::from_secs(integer(value).max(0) as u64);
            }
            "opcache.revalidate_path" => self.revalidate_path = boolean(value),
            _ => {}
        }
    }
}

#[cfg(unix)]
pub(crate) fn path_spelling(bytes: &[u8]) -> std::borrow::Cow<'_, OsStr> {
    use std::os::unix::ffi::OsStrExt;
    std::borrow::Cow::Borrowed(OsStr::from_bytes(bytes))
}

#[cfg(not(unix))]
pub(crate) fn path_spelling(bytes: &[u8]) -> std::borrow::Cow<'_, OsStr> {
    match String::from_utf8_lossy(bytes) {
        std::borrow::Cow::Borrowed(text) => std::borrow::Cow::Borrowed(OsStr::new(text)),
        std::borrow::Cow::Owned(text) => std::borrow::Cow::Owned(text.into()),
    }
}

/// Resolves PHP includes through the embedding runtime code cache.
pub trait CodeLoader {
    fn load(&self, path: PathBuf) -> LocalFuture<'_, Result<Arc<CompiledUnit>, CodeLoadError>>;

    fn wraps_include(&self, _context: &mut dyn Context, _path: &[u8]) -> bool {
        false
    }

    /// A request stream can provide source without going through the filesystem cache.
    fn load_stream<'a>(
        &'a self,
        _context: &'a mut dyn Context,
        _path: &'a [u8],
        _operation: &'a str,
        _once: bool,
    ) -> LocalFuture<'a, Result<StreamInclude, NativeError>> {
        Box::pin(std::future::ready(Ok(StreamInclude::LocalFile)))
    }

    fn load_include<'a>(
        &'a self,
        lookup: IncludeLookup<'a>,
        _policy: IncludeCachePolicy,
    ) -> IncludeLoad<'a> {
        IncludeLoad::Pending(Box::pin(async move {
            let mut last = CodeLoadError::Unavailable(String::new());
            for candidate in lookup.candidates() {
                match self.load(candidate).await {
                    Err(error @ CodeLoadError::Unavailable(_)) => last = error,
                    result => return result,
                }
            }
            Err(last)
        }))
    }
}

pub enum StreamInclude {
    LocalFile,
    Unavailable,
    AlreadyIncluded,
    Compiled(Result<Arc<CompiledUnit>, phera_compiler::CompileError>),
}

/// A cached include returns directly. Only unresolved paths need a future.
pub enum IncludeLoad<'a> {
    Cached(Arc<CompiledUnit>),
    Pending(LocalFuture<'a, Result<Arc<CompiledUnit>, CodeLoadError>>),
}

impl IncludeLoad<'_> {
    pub(crate) async fn finish(self) -> Result<Arc<CompiledUnit>, CodeLoadError> {
        match self {
            Self::Cached(unit) => Ok(unit),
            Self::Pending(load) => load.await,
        }
    }
}

/// PHP retries missing include paths, while compilation errors abort that lookup.
pub enum CodeLoadError {
    Unavailable(String),
    Compile(phera_compiler::CompileError),
}

impl<T: CodeLoader + ?Sized> CodeLoader for Arc<T> {
    fn load(&self, path: PathBuf) -> LocalFuture<'_, Result<Arc<CompiledUnit>, CodeLoadError>> {
        self.as_ref().load(path)
    }

    fn wraps_include(&self, context: &mut dyn Context, path: &[u8]) -> bool {
        self.as_ref().wraps_include(context, path)
    }

    fn load_stream<'a>(
        &'a self,
        context: &'a mut dyn Context,
        path: &'a [u8],
        operation: &'a str,
        once: bool,
    ) -> LocalFuture<'a, Result<StreamInclude, NativeError>> {
        self.as_ref().load_stream(context, path, operation, once)
    }

    fn load_include<'a>(
        &'a self,
        lookup: IncludeLookup<'a>,
        policy: IncludeCachePolicy,
    ) -> IncludeLoad<'a> {
        self.as_ref().load_include(lookup, policy)
    }
}
