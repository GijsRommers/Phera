//! Optional names for functions that expose PHP's dynamic variable scope.

use std::collections::HashMap;

use phera_bytecode::Reg;

/// Only functions with observable dynamic scope allocate their name table eagerly.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum VariableScope {
    Static,
    Dynamic,
}

impl From<bool> for VariableScope {
    fn from(dynamic: bool) -> Self {
        if dynamic { Self::Dynamic } else { Self::Static }
    }
}

/// Functions with static scope have no dynamic name table.
pub(crate) struct DynamicVariables {
    names: Option<Box<DynamicNames>>,
}

/// The optional table keeps dynamic scope out of ordinary activation storage.
struct DynamicNames {
    values: HashMap<String, Reg>,
}

impl DynamicVariables {
    pub(crate) fn new(scope: VariableScope) -> Self {
        Self {
            names: (scope == VariableScope::Dynamic).then(|| {
                Box::new(DynamicNames {
                    values: HashMap::new(),
                })
            }),
        }
    }

    /// Reuse a dynamic scope's name table after releasing its previous invocation's names.
    pub(crate) fn reset(&mut self, scope: VariableScope) {
        match (scope, &mut self.names) {
            (VariableScope::Static, _) => self.names = None,
            (VariableScope::Dynamic, Some(names)) => names.values.clear(),
            (VariableScope::Dynamic, None) => *self = Self::new(scope),
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&Reg> {
        self.names.as_ref()?.values.get(name)
    }

    /// Dynamic native calls can introduce names without a static scope flag.
    pub(crate) fn insert(&mut self, name: String, register: Reg) {
        self.names
            .get_or_insert_with(|| {
                Box::new(DynamicNames {
                    values: HashMap::new(),
                })
            })
            .values
            .insert(name, register);
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &Reg)> {
        self.names.iter().flat_map(|names| names.values.iter())
    }
}
