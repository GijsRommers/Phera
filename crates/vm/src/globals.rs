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

    pub(crate) fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)
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
            .filter(|(_, value)| sole_object_owner(value))
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Drops the global `name` when it is this very cell: the main script
    /// unset the variable that a function had bound with `global` or
    /// `$GLOBALS`, so the global is gone, as in PHP.
    pub(crate) fn forget_cell(&mut self, name: &str, cell: &Rc<RefCell<Value>>) {
        if matches!(self.values.get(name), Some(Value::Reference(held)) if Rc::ptr_eq(held, cell)) {
            self.values.remove(name);
        }
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

/// Whether `value` is the only holder of an object. A global that only the
/// globals table binds (as `$GLOBALS['name'] = ...` makes one) is a PHP
/// reference held once, which PHP's symbol table would hold as a plain value.
fn sole_object_owner(value: &Value) -> bool {
    match value {
        Value::Object(object) => Rc::strong_count(object) == 1,
        Value::Reference(cell) if Rc::strong_count(cell) == 1 => cell.try_borrow().is_ok_and(
            |inner| matches!(&*inner, Value::Object(object) if Rc::strong_count(object) == 1),
        ),
        _ => false,
    }
}
