//! Request-local globals state kept separate from register execution.

use std::collections::HashMap;

use phera_value::Value;

use crate::references::StaticKey;

/// Owns the request state for globals.
pub(crate) struct Globals {
    /// Request-owned globals.
    pub(crate) globals: HashMap<String, Value>,
    /// Request-owned statics.
    pub(crate) statics: HashMap<StaticKey, Value>,
}

impl Globals {
    pub(crate) fn new() -> Self {
        Self {
            globals: HashMap::new(),
            statics: HashMap::new(),
        }
    }
}
