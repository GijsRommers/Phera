//! `$ok = $ready && set_output($out); if ($ok) { echo $out; }` defines the output.

use super::{Definition, Place, Scope};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub(crate) struct DefinitionGuards {
    guards: BTreeMap<GuardKey, Guard>,
    stable_roots: BTreeMap<usize, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GuardKey {
    Variable(String),
    EnumCase {
        place: Place,
        class: String,
        case: String,
    },
}

impl GuardKey {
    pub(crate) fn root(&self) -> &str {
        match self {
            Self::Variable(name) => name,
            Self::EnumCase { place, .. } => &place.root,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Guard {
    yes: BTreeSet<String>,
    no: BTreeSet<String>,
}

impl Scope {
    pub(crate) fn remember_definitions(
        &mut self,
        guard: GuardKey,
        yes: Option<&Scope>,
        no: Option<&Scope>,
    ) {
        let guard_paths = Guard {
            yes: extra_definitions(self, yes),
            no: extra_definitions(self, no),
        };
        if !guard_paths.yes.is_empty() || !guard_paths.no.is_empty() {
            self.definition_guards.guards.insert(guard, guard_paths);
        }
    }

    pub(crate) fn track_guard_root(&mut self, offset: usize, root: &str) {
        self.definition_guards
            .stable_roots
            .insert(offset, root.into());
    }

    pub(crate) fn guard_root_unchanged(&self, offset: usize) -> bool {
        self.definition_guards.stable_roots.contains_key(&offset)
    }

    pub(crate) fn finish_guard(&mut self, offset: usize) {
        self.definition_guards.stable_roots.remove(&offset);
    }

    pub(crate) fn refine_variable_guard(&mut self, name: &str, truth: bool) {
        if !self.definition_guards.guards.is_empty() {
            self.refine_definitions(&GuardKey::Variable(name.into()), truth);
        }
    }

    pub(crate) fn refine_definitions(&mut self, guard: &GuardKey, truth: bool) {
        if let Some(paths) = self.definition_guards.guards.get(guard) {
            let names = if truth { &paths.yes } else { &paths.no };
            for name in names {
                if let Some(variable) = self.variables.get_mut(name) {
                    variable.definition = Definition::Definite;
                }
            }
        }
    }
}

impl DefinitionGuards {
    pub fn reassign(&mut self, name: &str) {
        self.guards.retain(|guard, _| guard.root() != name);
        self.stable_roots.retain(|_, root| root != name);
    }

    pub fn forget(&mut self, name: &str) {
        self.reassign(name);
        self.guards.retain(|_, paths| {
            paths.yes.remove(name);
            paths.no.remove(name);
            !paths.yes.is_empty() || !paths.no.is_empty()
        });
    }

    pub fn forget_property_write(&mut self, place: Option<&Place>) {
        self.guards.retain(|guard, _| match guard {
            GuardKey::Variable(_) => true,
            GuardKey::EnumCase { place: guard, .. } => {
                !super::facts::affected_by_write(guard, place)
            }
        });
    }

    pub fn retain_common(&mut self, scopes: &[Scope]) {
        self.stable_roots.retain(|offset, root| {
            scopes
                .iter()
                .all(|scope| scope.definition_guards.stable_roots.get(offset) == Some(root))
        });
        self.guards.retain(|name, paths| {
            scopes
                .iter()
                .all(|scope| scope.definition_guards.guards.get(name) == Some(paths))
        });
    }
}

fn extra_definitions(joined: &Scope, path: Option<&Scope>) -> BTreeSet<String> {
    path.into_iter()
        .flat_map(|scope| &scope.variables)
        .filter(|(name, value)| {
            value.definition == Definition::Definite
                && joined
                    .variables
                    .get(*name)
                    .is_some_and(|value| value.definition != Definition::Definite)
        })
        .map(|(name, _)| name.clone())
        .collect()
}
