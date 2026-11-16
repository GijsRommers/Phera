//! Optional names for functions that expose PHP's dynamic variable scope.

use std::collections::HashMap;

use phera_bytecode::Reg;

/// Functions with static scope have no dynamic name table.
pub(crate) struct DynamicVariables {
    names: Option<HashMap<String, Reg>>,
}

impl DynamicVariables {
    pub(crate) fn new(enabled: bool) -> Self {
        Self {
            names: enabled.then(HashMap::new),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Reg> {
        self.names.as_ref()?.get(name)
    }

    /// Dynamic native calls can introduce names without a static scope flag.
    pub(crate) fn insert(&mut self, name: String, register: Reg) {
        self.names
            .get_or_insert_with(HashMap::new)
            .insert(name, register);
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &Reg)> {
        self.names.iter().flat_map(|names| names.iter())
    }
}
