//! Project declarations, including inherited members: `class A extends B {}`.
//! The index owns small signatures so body analysis never executes PHP code.

use crate::{names::Names, types::ValueType};
use phera_ast::{ClassKind, Modifiers, Parameter, Type, Visibility};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub file: String,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterInfo {
    pub name: String,
    pub ty: ValueType,
    pub optional: bool,
    pub variadic: bool,
    pub by_reference: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub parameters: Vec<ParameterInfo>,
    pub returns: ValueType,
    pub location: Location,
    pub modifiers: Modifiers,
    pub abstract_body: bool,
    pub accepts_extra: bool,
}

impl Signature {
    pub fn new(
        parameters: &[Parameter],
        returns: Option<&Type>,
        names: &Names,
        location: Location,
    ) -> Self {
        Self {
            parameters: parameters
                .iter()
                .map(|p| ParameterInfo {
                    name: p.name.clone(),
                    ty: ValueType::declared(p.ty.as_ref(), names),
                    optional: p.default.is_some(),
                    variadic: p.variadic,
                    by_reference: p.by_reference,
                })
                .collect(),
            returns: ValueType::declared(returns, names),
            location,
            modifiers: Modifiers::default(),
            abstract_body: false,
            accepts_extra: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PropertyInfo {
    pub ty: ValueType,
    pub modifiers: Modifiers,
    pub location: Location,
}

#[derive(Clone, Debug)]
pub struct ClassInfo {
    pub name: String,
    pub kind: ClassKind,
    pub modifiers: Modifiers,
    pub parents: Vec<String>,
    pub interfaces: Vec<String>,
    pub traits: Vec<String>,
    pub methods: BTreeMap<String, Signature>,
    pub properties: BTreeMap<String, PropertyInfo>,
    pub constants: BTreeMap<String, Location>,
    pub location: Location,
    pub open_properties: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Index {
    pub classes: BTreeMap<String, ClassInfo>,
    pub functions: BTreeMap<String, Signature>,
    pub constants: BTreeMap<String, Location>,
}

impl Index {
    pub fn class(&self, name: &str) -> Option<&ClassInfo> {
        self.classes.get(&name.to_lowercase())
    }

    pub fn ancestry(&self, name: &str) -> Vec<&ClassInfo> {
        let mut pending = vec![name.to_string()];
        let mut seen = BTreeSet::new();
        let mut result = Vec::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(name.to_lowercase()) {
                continue;
            }
            if let Some(class) = self.class(&name) {
                pending.extend(class.interfaces.iter().rev().cloned());
                pending.extend(class.parents.iter().rev().cloned());
                pending.extend(class.traits.iter().rev().cloned());
                result.push(class);
            }
        }
        result
    }

    pub fn method(&self, class: &str, method: &str) -> Option<&Signature> {
        self.ancestry(class)
            .into_iter()
            .find_map(|c| c.methods.get(&method.to_lowercase()))
    }

    pub fn property(&self, class: &str, property: &str) -> Option<&PropertyInfo> {
        self.ancestry(class)
            .into_iter()
            .find_map(|c| c.properties.get(property))
    }

    pub fn constant(&self, class: &str, constant: &str) -> bool {
        self.ancestry(class)
            .iter()
            .any(|c| c.constants.contains_key(constant))
    }

    pub fn open_properties(&self, class: &str) -> bool {
        self.ancestry(class).iter().any(|c| {
            c.open_properties || c.methods.contains_key("__get") || c.methods.contains_key("__set")
        })
    }

    pub fn public_visibility() -> Visibility {
        Visibility::Public
    }
}
