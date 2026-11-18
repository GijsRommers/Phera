//! Request globals and the shared cells created by PHP global declarations.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use phera_value::Value;

/// Global values are promoted to reference cells when a function binds them.
pub(crate) struct Globals {
    values: HashMap<String, Value>,
}

impl Globals {
    pub(crate) fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, name: String, value: Value) {
        self.values.insert(name, value);
    }

    /// Unset detaches the global name while existing references remain alive.
    pub(crate) fn remove(&mut self, name: &str) {
        self.values.remove(name);
    }

    /// How many globals exist outside the main script's own variables.
    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    /// The globals that hold an object nobody else holds, for the end of
    /// the request (see `crate::request_end`).
    pub(crate) fn sole_object_owners(&self) -> Vec<String> {
        self.values
            .iter()
            .filter(|(_, value)| matches!(value, Value::Object(object) if Rc::strong_count(object) == 1))
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
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
