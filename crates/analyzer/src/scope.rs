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
    pub external_references: BTreeSet<String>,
}

impl Scope {
    pub fn define(&mut self, name: &str, ty: ValueType) {
        self.values.remove(name);
        self.known.retain(|symbol| !symbol.refers_to(name));
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
                self.known.retain(|symbol| !symbol.refers_to(&alias));
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
        self.known
            .retain(|symbol| !matches!(symbol, Symbol::Property(..) | Symbol::Method(..)));
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

    pub fn detach(&mut self, name: &str) {
        self.aliases.remove(name);
        self.external_references.remove(name);
        for aliases in self.aliases.values_mut() {
            aliases.remove(name);
        }
    }

    pub fn reference(&mut self, left: &str, right: &str) {
        if left == right {
            return;
        }
        self.detach(left);
        let mut names = self.aliases.get(right).cloned().unwrap_or_default();
        names.extend([left.into(), right.into()]);
        if names
            .iter()
            .any(|name| self.external_references.contains(name))
        {
            self.external_references.extend(names.iter().cloned());
        }
        for name in &names {
            self.aliases.insert(name.clone(), names.clone());
        }
    }

    pub fn forget_reference_types(&mut self) {
        let names: Vec<_> = self.external_references.iter().cloned().collect();
        for name in names {
            let definition = self.get(&name).definition;
            self.define(&name, ValueType::Mixed);
            if let Some(variable) = self.variables.get_mut(&name) {
                variable.definition = definition;
            }
        }
        if self.dynamic {
            self.unknown_writes();
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
        result.external_references = paths
            .iter()
            .flat_map(|p| p.external_references.iter().cloned())
            .collect();
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
    Property(String, String),
    Method(String, String),
}

impl Symbol {
    fn refers_to(&self, receiver: &str) -> bool {
        matches!(self, Self::Property(name, _) | Self::Method(name, _) if name == receiver)
    }

    pub fn constant_name(name: &str) -> String {
        let name = name.trim_start_matches('\\');
        name.split_once("::").map_or_else(
            || name.to_string(),
            |(class, constant)| format!("{}::{constant}", class.to_ascii_lowercase()),
        )
    }
}
