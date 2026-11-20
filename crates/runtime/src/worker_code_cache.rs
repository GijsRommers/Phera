//! For a repeated `require __DIR__ . '/autoload.php'`, a worker can return
//! its resolved compiled file without entering the blocking pool.

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use phera_bytecode::CompiledUnit;
use phera_extension_api::LocalFuture;
use phera_vm::{
    CodeLoadError, CodeLoader, IncludeCachePolicy, IncludeLoad, IncludeLookup, RelativeInclude,
};

use crate::RuntimeError;
use crate::cache::{CodeCache, Validation, include_error};

const MAX_SPELLINGS: usize = 8192;

pub(crate) struct WorkerCodeCache {
    shared: CodeCache,
    absolute: RefCell<HashMap<OsString, ResolvedFile>>,
    relative: RefCell<HashMap<RelativeInclude, ResolvedFile>>,
}

pub(crate) struct RequestCodeCache {
    worker: Rc<WorkerCodeCache>,
    started: Instant,
}

struct ResolvedFile {
    unit: Arc<CompiledUnit>,
    resolved_at: Instant,
    validated_at: Option<Instant>,
}

impl WorkerCodeCache {
    pub(crate) fn new(shared: CodeCache) -> Self {
        Self {
            shared,
            absolute: RefCell::default(),
            relative: RefCell::default(),
        }
    }
}

impl RequestCodeCache {
    pub(crate) fn new(worker: Rc<WorkerCodeCache>) -> Self {
        Self {
            worker,
            started: Instant::now(),
        }
    }

    pub(crate) async fn entry(
        &self,
        script: &Path,
        policy: IncludeCachePolicy,
    ) -> Result<Arc<CompiledUnit>, RuntimeError> {
        let lookup = IncludeLookup::Absolute(script.as_os_str());
        if let Some(unit) = self.cached_include(&lookup, policy) {
            return Ok(unit);
        }
        let unit = self
            .worker
            .shared
            .compile_candidates(vec![script.to_path_buf()], validation(policy))
            .await?;
        self.remember_include(lookup, &unit, policy);
        Ok(unit)
    }

    fn fresh(&self, entry: &ResolvedFile, policy: IncludeCachePolicy) -> bool {
        !policy.validate_timestamps
            || entry.validated_at.is_some_and(|checked_at| {
                self.started.saturating_duration_since(checked_at) <= policy.revalidate_freq
            })
    }

    fn cached_include(
        &self,
        lookup: &IncludeLookup<'_>,
        policy: IncludeCachePolicy,
    ) -> Option<Arc<CompiledUnit>> {
        if policy.revalidate_path {
            return None;
        }
        match lookup {
            IncludeLookup::Absolute(path) => self.lookup(&self.worker.absolute, *path, policy),
            IncludeLookup::Relative(key) => self.lookup(&self.worker.relative, key, policy),
        }
    }

    fn lookup<Key: Borrow<Query> + Eq + Hash, Query: Eq + Hash + ?Sized>(
        &self,
        entries: &RefCell<HashMap<Key, ResolvedFile>>,
        key: &Query,
        policy: IncludeCachePolicy,
    ) -> Option<Arc<CompiledUnit>> {
        {
            let entries = entries.borrow();
            let entry = entries.get(key)?;
            if self.fresh(entry, policy) {
                return Some(entry.unit.clone());
            }
        }
        // A failed recheck must not leave an old answer to resurrect if
        // a later request changes or disables timestamp validation.
        entries.borrow_mut().remove(key);
        None
    }

    fn remember_include(
        &self,
        lookup: IncludeLookup<'_>,
        unit: &Arc<CompiledUnit>,
        policy: IncludeCachePolicy,
    ) {
        let entry = ResolvedFile {
            unit: unit.clone(),
            resolved_at: self.started,
            validated_at: policy.validate_timestamps.then_some(self.started),
        };
        match lookup {
            IncludeLookup::Absolute(path) => remember(
                &mut self.worker.absolute.borrow_mut(),
                path.to_owned(),
                entry,
            ),
            IncludeLookup::Relative(key) => {
                remember(&mut self.worker.relative.borrow_mut(), key, entry)
            }
        }
    }
}

impl CodeLoader for RequestCodeCache {
    fn wraps_include(&self, context: &mut dyn phera_extension_api::Context, path: &[u8]) -> bool {
        phera_ext_streams::wraps_include(context, path)
    }

    fn load_stream<'a>(
        &'a self,
        context: &'a mut dyn phera_extension_api::Context,
        path: &'a [u8],
        operation: &'a str,
        once: bool,
    ) -> LocalFuture<'a, Result<phera_vm::StreamInclude, phera_extension_api::NativeError>> {
        self.worker
            .shared
            .load_stream(context, path, operation, once)
    }

    fn load(&self, path: PathBuf) -> LocalFuture<'_, Result<Arc<CompiledUnit>, CodeLoadError>> {
        Box::pin(async move {
            self.entry(&path, IncludeCachePolicy::default())
                .await
                .map_err(include_error)
        })
    }

    fn load_include<'a>(
        &'a self,
        lookup: IncludeLookup<'a>,
        policy: IncludeCachePolicy,
    ) -> IncludeLoad<'a> {
        if let Some(unit) = self.cached_include(&lookup, policy) {
            return IncludeLoad::Cached(unit);
        }
        IncludeLoad::Pending(Box::pin(async move {
            let unit = self
                .worker
                .shared
                .compile_candidates(lookup.candidates(), validation(policy))
                .await
                .map_err(include_error)?;
            self.remember_include(lookup, &unit, policy);
            Ok(unit)
        }))
    }
}

fn remember<Key: Eq + Hash>(
    entries: &mut HashMap<Key, ResolvedFile>,
    key: Key,
    entry: ResolvedFile,
) {
    if let Some(existing) = entries.get_mut(&key) {
        // Includes may finish out of order while a worker awaits blocking work.
        if existing.resolved_at <= entry.resolved_at {
            *existing = entry;
        }
    } else if entries.len() < MAX_SPELLINGS {
        entries.insert(key, entry);
    }
}

fn validation(policy: IncludeCachePolicy) -> Validation {
    if policy.validate_timestamps {
        Validation::Timestamps
    } else {
        Validation::Frozen
    }
}
