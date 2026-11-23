//! Function static variables retain their reference identity between calls.

use std::collections::HashMap;

use phera_value::Value;

use crate::references::StaticKey;

/// Storage is separate from globals: a static belongs to one compiled function.
pub(crate) struct StaticVariables {
    values: HashMap<StaticKey, Value>,
}

impl StaticVariables {
    pub(crate) fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    pub(crate) fn get(&self, key: &StaticKey) -> Option<&Value> {
        self.values.get(key)
    }

    /// The caller supplies a reference cell shared with the initializing local.
    pub(crate) fn insert(&mut self, key: StaticKey, value: Value) {
        self.values.insert(key, value);
    }
}

impl crate::snapshot::ForkCopy for StaticVariables {
    fn fork_copy(&self, fork: &mut phera_value::Fork<'_>) -> Self {
        StaticVariables {
            values: self.values.fork_copy(fork),
        }
    }
}
