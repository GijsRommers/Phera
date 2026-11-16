//! Request-local function table state kept separate from register execution.

use std::{collections::HashMap, sync::Arc};

use phera_bytecode::CompiledUnit;

use crate::SymbolCache;

/// Owns the request state for function table.
pub(crate) struct FunctionTable {
    /// Request-owned functions.
    pub(crate) functions: HashMap<String, FunctionRef>,
    /// Request-owned code caches.
    pub(crate) code_caches: HashMap<usize, SymbolCache>,
}

impl FunctionTable {
    pub(crate) fn new() -> Self {
        Self {
            functions: HashMap::new(),
            code_caches: HashMap::new(),
        }
    }
}

/// A declared PHP function and the immutable unit that owns its instructions.
#[derive(Clone)]
pub(crate) struct FunctionRef {
    pub(crate) unit: Arc<CompiledUnit>,
    pub(crate) index: usize,
}
