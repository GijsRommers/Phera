//! Request globals keep their symbol order while their values live in registers.

use std::{cell::RefCell, rc::Rc};

use foldhash::fast::RandomState;
use indexmap::IndexMap;
use phera_value::Value;

/// A vacant entry reserves the symbol's place while its value is in a frame.
pub(crate) struct Globals {
    values: IndexMap<String, Option<Value>, RandomState>,
}

impl Globals {
    pub(crate) fn new() -> Self {
        Self {
            values: IndexMap::with_hasher(RandomState::default()),
        }
    }

    pub(crate) fn declare(&mut self, name: &str) {
        if !self.values.contains_key(name) {
            self.values.insert(name.to_owned(), None);
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Value> {
        self.values.get(name)?.as_ref()
    }

    pub(crate) fn insert(&mut self, name: String, value: Value) {
        self.values.insert(name, Some(value));
    }

    /// Move a value to a frame without changing the symbol's insertion order.
    pub(crate) fn take(&mut self, name: &str) -> Option<Value> {
        self.values.get_mut(name)?.take()
    }

    /// Unset detaches the global name while existing references remain alive.
    pub(crate) fn remove(&mut self, name: &str) {
        self.values.shift_remove(name);
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = (&String, &Value)> {
        self.values
            .iter()
            .filter_map(|(name, value)| Some((name, value.as_ref()?)))
    }

    /// At shutdown PHP visits sole-owned objects from the last symbol to the first.
    pub(crate) fn sole_object_owners(&self) -> Vec<String> {
        self.iter()
            .rev()
            .filter(|(_, value)| sole_object_owner(value))
            .map(|(name, _)| name.clone())
            .collect()
    }

    pub(crate) fn release_sole_object(&mut self, name: &str) {
        if self.get(name).is_some_and(sole_object_owner) {
            self.remove(name);
        }
    }

    /// Where symbol `name` is in the table, which stays its place until it is
    /// unset.
    pub(crate) fn index_of(&self, name: &str) -> Option<usize> {
        self.values.get_index_of(name)
    }

    /// The PHP reference that global `name` holds, when it is still at
    /// `index` and its value is in the table, not in a frame.
    #[inline]
    pub(crate) fn reference_at(&self, index: usize, name: &[u8]) -> Option<&Rc<RefCell<Value>>> {
        match self.values.get_index(index)? {
            (key, Some(Value::Reference(cell))) if key.as_bytes() == name => Some(cell),
            _ => None,
        }
    }

    /// Repeated global declarations share the same cell, including a missing global.
    pub(crate) fn reference(&mut self, name: String) -> Value {
        let value = self
            .values
            .entry(name)
            .or_default()
            .get_or_insert(Value::Null);
        if !value.is_reference() {
            *value = Value::Reference(Rc::new(RefCell::new(std::mem::take(value))));
        }
        value.clone()
    }
}

/// PHP leaves aliases, arrays and shared objects for the object-store pass.
fn sole_object_owner(value: &Value) -> bool {
    matches!(value, Value::Object(object) if Rc::strong_count(object) == 1)
}

impl crate::snapshot::ForkCopy for Globals {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        Globals {
            values: self.values.fork_copy(fork),
        }
    }
}
