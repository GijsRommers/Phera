//! Borrow immutable bytecode paths through an owned code handle.
//!
//! Cloning a handle increments only the unit Arc. Ordinary path execution never
//! allocates a second vector of register steps. Foreach adds its current key to
//! the handle without copying the bytecode path.

use std::sync::Arc;

use phera_bytecode::{CompiledUnit, Path, PathBase, PathId, Reg, Step};
use phera_extension_api::NativeError;

/// The source of a path; plain local arguments have no table entry.
#[derive(Clone)]
enum PathSource {
    Local(Reg),
    Compiled {
        unit: Arc<CompiledUnit>,
        function: Option<usize>,
        index: PathId,
    },
}

/// An immutable bytecode path plus an optional live foreach key.
#[derive(Clone)]
pub(crate) struct AccessPath {
    source: PathSource,
    key: Option<Step>,
}

impl AccessPath {
    /// A local passed as an argument needs no allocated path table entry.
    pub(crate) fn local(register: Reg) -> Self {
        Self {
            source: PathSource::Local(register),
            key: None,
        }
    }

    /// Keep the code alive independently of mutable register access.
    pub(crate) fn compiled(
        unit: Arc<CompiledUnit>,
        function: Option<usize>,
        index: PathId,
    ) -> Self {
        Self {
            source: PathSource::Compiled {
                unit,
                function,
                index,
            },
            key: None,
        }
    }

    /// Borrow a compiled path while the access handle keeps its owning unit alive.
    fn definition(&self) -> Result<Option<&Path>, NativeError> {
        let PathSource::Compiled {
            unit,
            function,
            index,
        } = &self.source
        else {
            return Ok(None);
        };
        let function = match function {
            Some(index) => unit.functions.get(*index),
            None => Some(&unit.main),
        };
        function
            .and_then(|function| function.paths.get(index.index()))
            .map(Some)
            .ok_or_else(|| internal_error!("invalid path"))
    }

    /// Resolve the root without evaluating or cloning any PHP values.
    pub(crate) fn base(&self) -> Result<PathBase, NativeError> {
        if let PathSource::Local(register) = self.source {
            return Ok(PathBase::Register(register));
        }
        self.definition()?
            .map(|path| path.base)
            .ok_or_else(|| internal_error!("missing path"))
    }

    /// Walk the original bytecode steps, followed by the live iterator key.
    pub(crate) fn steps(&self) -> Result<impl Iterator<Item = Step> + '_, NativeError> {
        let steps = self
            .definition()?
            .map_or(&[][..], |path| path.steps.as_slice());
        Ok(steps.iter().copied().chain(self.key))
    }

    /// Add a current foreach key; the bytecode path itself stays immutable.
    pub(crate) fn with_key(mut self, key: Reg) -> Self {
        self.key = Some(Step::Index(key));
        self
    }

    /// Specialized append instructions retain the ordinary path fallback.
    pub(crate) fn with_append(mut self) -> Self {
        self.key = Some(Step::Append);
        self
    }
}
