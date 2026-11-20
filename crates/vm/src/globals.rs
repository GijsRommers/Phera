//! Request globals and the shared cells created by PHP global declarations.

use std::{cell::RefCell, rc::Rc};

use indexmap::IndexMap;
use phera_value::Value;

/// Global values are promoted to reference cells when a function binds them.
pub(crate) struct Globals {
    values: IndexMap<String, Value>,
}

impl Globals {
    pub(crate) fn new() -> Self {
        Self {
            values: IndexMap::new(),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
    }

    pub(crate) fn insert(&mut self, name: String, value: Value) {
        self.values.insert(name, value);
    }

    pub(crate) fn take(&mut self, name: &str) -> Option<Value> {
        self.values.shift_remove(name)
    }

    /// Unset detaches the global name while existing references remain alive.
    pub(crate) fn remove(&mut self, name: &str) {
        self.values.shift_remove(name);
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = (&String, &Value)> {
        self.values.iter()
    }

    /// Repeated global declarations share the same cell, including a missing global.
    pub(crate) fn reference(&mut self, name: String) -> Value {
        let value = self.values.entry(name).or_default();
        if !value.is_reference() {
            *value = Value::Reference(Rc::new(RefCell::new(std::mem::take(value))));
        }
        value.clone()
    }
}
