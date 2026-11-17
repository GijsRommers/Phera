//! Request-local function table state kept separate from register execution.

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use phera_bytecode::CompiledUnit;

/// Owns the request state for function table.
pub(crate) struct FunctionTable {
    /// Request-owned functions.
    functions: HashMap<String, FunctionRef>,
    /// Raw callable names retain resolved signatures until the declaration table changes.
    resolved: RefCell<HashMap<phera_value::PhpString, crate::calls::call_state::Callee>>,
}

impl FunctionTable {
    pub(crate) fn new() -> Self {
        Self {
            functions: HashMap::new(),
            resolved: RefCell::new(HashMap::new()),
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
        self.resolved.get_mut().clear();
        self.functions.insert(lowercase_name, function);
    }
}

impl FunctionTable {
    /// Byte identity includes casing and namespace prefixes; resolution handles those on a miss.
    pub(crate) fn resolved(
        &self,
        name: &phera_value::PhpString,
    ) -> Option<crate::calls::call_state::Callee> {
        self.resolved.borrow().get(name).cloned()
    }

    /// Insertion invalidates this request-local cache, so later declarations cannot be hidden.
    pub(crate) fn cache_resolution(
        &self,
        name: &phera_value::PhpString,
        callee: &crate::calls::call_state::Callee,
    ) {
        self.resolved
            .borrow_mut()
            .insert(name.clone(), callee.clone());
    }
}
