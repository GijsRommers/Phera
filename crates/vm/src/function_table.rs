//! Request-local function table state kept separate from register execution.

use std::{cell::RefCell, rc::Rc};

use foldhash::{HashMap, HashMapExt};

use crate::code_table::FunctionIndex;

/// Owns the request state for function table.
pub(crate) struct FunctionTable {
    /// Request-owned functions, shared copy-on-write across snapshot forks.
    functions: Rc<HashMap<String, FunctionIndex>>,
    order: Rc<Vec<FunctionIndex>>,
    /// Raw callable names retain resolved signatures until the declaration table changes.
    resolved: RefCell<HashMap<phera_value::PhpString, crate::calls::call_state::Callee>>,
}

impl FunctionTable {
    pub(crate) fn new() -> Self {
        Self {
            functions: Rc::new(HashMap::new()),
            order: Rc::new(Vec::new()),
            resolved: RefCell::new(HashMap::new()),
        }
    }
}

impl FunctionTable {
    /// Callers normalize names once; PHP function names are case-insensitive.
    pub(crate) fn get(&self, lowercase_name: &str) -> Option<FunctionIndex> {
        self.functions.get(lowercase_name).copied()
    }

    pub(crate) fn contains(&self, lowercase_name: &str) -> bool {
        self.functions.contains_key(lowercase_name)
    }

    /// Redeclaration checks stay at the caller because main and include differ.
    pub(crate) fn insert(&mut self, lowercase_name: String, function: FunctionIndex) {
        self.resolved.get_mut().clear();
        Rc::make_mut(&mut self.functions).insert(lowercase_name, function);
        Rc::make_mut(&mut self.order).push(function);
    }

    pub(crate) fn declaration_order(&self) -> impl Iterator<Item = FunctionIndex> + '_ {
        self.order.iter().copied()
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

impl FunctionTable {
    /// A fork declares the same functions; resolved callables are a cache it
    /// fills again.
    pub(crate) fn forked(&self) -> Self {
        Self {
            functions: self.functions.clone(),
            order: self.order.clone(),
            resolved: RefCell::new(HashMap::new()),
        }
    }
}
