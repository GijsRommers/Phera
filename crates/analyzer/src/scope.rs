//! Definite assignment: `if ($flag) { $x = 1; }` only possibly defines `$x`.
//! Dynamic writes keep uncertain reads silent instead of guessing variable names.

use crate::types::ValueType;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Definition {
    Definite,
    Possible,
    Absent,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct VariableInfo {
    pub definition: Definition,
    pub ty: ValueType,
}

#[derive(Clone, Debug, Default)]
pub struct Scope {
    pub known: BTreeSet<Symbol>,
    pub values: BTreeMap<String, crate::constants::ConstantValue>,
    pub variables: BTreeMap<String, VariableInfo>,
    pub dynamic: bool,
    pub aliases: BTreeMap<String, BTreeSet<String>>,
}

impl Scope {
    pub fn define(&mut self, name: &str, ty: ValueType) {
        self.values.remove(name);
        self.variables.insert(
            name.into(),
            VariableInfo {
                definition: Definition::Definite,
                ty: ty.clone(),
            },
        );
        if let Some(aliases) = self.aliases.get(name).cloned() {
            for alias in aliases {
                self.values.remove(&alias);
                self.variables.insert(
                    alias,
                    VariableInfo {
                        definition: Definition::Definite,
                        ty: ty.clone(),
                    },
                );
            }
        }
    }

    pub fn unknown_writes(&mut self) {
        self.dynamic = true;
        self.values.clear();
        for variable in self.variables.values_mut() {
            variable.ty = ValueType::Mixed;
        }
    }

    pub fn get(&self, name: &str) -> VariableInfo {
        if matches!(
            name,
            "GLOBALS"
                | "_SERVER"
                | "_GET"
                | "_POST"
                | "_FILES"
                | "_COOKIE"
                | "_SESSION"
                | "_REQUEST"
                | "_ENV"
        ) {
            return VariableInfo {
                definition: Definition::Definite,
                ty: ValueType::Array,
            };
        }
        self.variables.get(name).cloned().unwrap_or(VariableInfo {
            definition: if self.dynamic {
                Definition::Possible
            } else {
                Definition::Absent
            },
            ty: ValueType::Mixed,
        })
    }

    pub fn reference(&mut self, left: &str, right: &str) {
        let mut names = BTreeSet::from([left.into(), right.into()]);
        names.extend(self.aliases.get(left).into_iter().flatten().cloned());
        names.extend(self.aliases.get(right).into_iter().flatten().cloned());
        for name in &names {
            self.aliases.insert(name.clone(), names.clone());
        }
    }

    pub fn join(paths: &[Self]) -> Self {
        let Some(first) = paths.first() else {
            return Self::default();
        };
        let mut result = first.clone();
        let keys: BTreeSet<_> = paths
            .iter()
            .flat_map(|p| p.variables.keys().cloned())
            .collect();
        result.dynamic = paths.iter().any(|p| p.dynamic);
        result
            .values
            .retain(|key, value| paths.iter().all(|p| p.values.get(key) == Some(value)));
        result
            .known
            .retain(|symbol| paths.iter().all(|p| p.known.contains(symbol)));
        result
            .aliases
            .retain(|key, value| paths.iter().all(|p| p.aliases.get(key) == Some(value)));
        for key in keys {
            let values: Vec<_> = paths.iter().map(|p| p.get(&key)).collect();
            let definite = values.iter().all(|v| v.definition == Definition::Definite);
            let ty = values
                .iter()
                .map(|v| v.ty.clone())
                .reduce(|a, b| a.join(&b))
                .unwrap_or(ValueType::Mixed);
            result.variables.insert(
                key,
                VariableInfo {
                    definition: if definite {
                        Definition::Definite
                    } else {
                        Definition::Possible
                    },
                    ty,
                },
            );
        }
        result
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Symbol {
    Class(String),
    Function(String),
    Constant(String),
}
