//! Request-local function table state kept separate from register execution.

use std::{collections::HashMap, sync::Arc};

use phera_bytecode::CompiledUnit;

/// Owns the request state for function table.
pub(crate) struct FunctionTable {
    /// Request-owned functions.
    functions: HashMap<String, FunctionRef>,
}

impl FunctionTable {
    pub(crate) fn new() -> Self {
        Self {
            functions: HashMap::new(),
        }
    }
}

/// A declared PHP function and the immutable unit that owns its instructions.
#[derive(Clone)]
pub(crate) struct FunctionRef {
    pub(crate) unit: Arc<CompiledUnit>,
    pub(crate) index: usize,
}

impl FunctionTable {
    /// Callers normalize names once; PHP function names are case-insensitive.
    pub(crate) fn get(&self, lowercase_name: &str) -> Option<&FunctionRef> {
        self.functions.get(lowercase_name)
    }

    pub(crate) fn contains(&self, lowercase_name: &str) -> bool {
        self.functions.contains_key(lowercase_name)
    }

    /// Redeclaration checks stay at the caller because main and include differ.
    pub(crate) fn insert(&mut self, lowercase_name: String, function: FunctionRef) {
        self.functions.insert(lowercase_name, function);
    }
}
