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
    pub closure_receiver: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub parameters: Vec<ParameterInfo>,
    pub returns: ValueType,
    pub location: Location,
    pub modifiers: Modifiers,
    pub abstract_body: bool,
    pub accepts_extra: bool,
    pub unbound_this: Vec<Location>,
    pub global_effects: crate::global_effects::GlobalEffects,
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
                    ty: ValueType::parameter(p, names),
                    optional: p.default.is_some(),
                    variadic: p.variadic,
                    by_reference: p.by_reference,
                    closure_receiver: None,
                })
                .collect(),
            returns: ValueType::declared(returns, names),
            location,
            modifiers: Modifiers::default(),
            abstract_body: false,
            accepts_extra: false,
            unbound_this: Vec::new(),
            global_effects: crate::global_effects::GlobalEffects::None,
        }
    }
    pub fn apply_phpdoc(&mut self, doc: Option<&str>, names: &Names) {
        if let Some(doc) = doc {
            if crate::phpdoc::never_returns(doc) {
                self.returns = ValueType::Never;
            }
            for parameter in &mut self.parameters {
                parameter.closure_receiver =
                    crate::phpdoc::closure_receiver(doc, &parameter.name, names);
            }
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
pub struct ConstantInfo {
    pub ty: ValueType,
    pub modifiers: Modifiers,
    pub location: Location,
}

impl ConstantInfo {
    pub fn untyped(location: Location) -> Self {
        Self {
            ty: ValueType::Mixed,
            modifiers: Modifiers::default(),
            location,
        }
    }
}

#[derive(Clone, Debug)]
pub enum MemberAdaptation {
    Alias {
        source: Option<String>,
        method: String,
        alias: String,
        visibility: Option<Visibility>,
    },
    Precedence {
        source: String,
        method: String,
    },
}

#[derive(Clone, Debug)]
pub struct ClassInfo {
    pub name: String,
    pub kind: ClassKind,
    pub modifiers: Modifiers,
    pub parents: Vec<String>,
    pub interfaces: Vec<String>,
    pub traits: Vec<String>,
    pub adaptations: Vec<MemberAdaptation>,
    pub methods: BTreeMap<String, Signature>,
    pub properties: BTreeMap<String, PropertyInfo>,
    pub constants: BTreeMap<String, ConstantInfo>,
    pub location: Location,
    pub open_properties: bool,
    pub documented: crate::phpdoc::Members,
}

#[derive(Clone, Debug, Default)]
pub struct Index {
    pub builtin_functions: BTreeSet<String>,
    pub builtin_classes: BTreeSet<String>,
    pub trait_users: crate::trait_users::TraitUsers,
    pub classes: BTreeMap<String, ClassInfo>,
    pub functions: BTreeMap<String, Signature>,
    pub constants: BTreeMap<String, ConstantInfo>,
}

impl Index {
    pub fn class(&self, name: &str) -> Option<&ClassInfo> {
        self.classes.get(&name.to_ascii_lowercase())
    }

    pub fn ancestry(&self, name: &str) -> Vec<&ClassInfo> {
        let mut pending = vec![name.to_string()];
        let mut seen = BTreeSet::new();
        let mut result = Vec::new();
        while let Some(name) = pending.pop() {
            if !seen.insert(name.to_ascii_lowercase()) {
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
        let mut contract = None;
        for ancestor in self.ancestry(class) {
            if let Some(signature) = ancestor.methods.get(&method.to_ascii_lowercase()) {
                if matches!(ancestor.kind, ClassKind::Trait | ClassKind::Interface)
                    && (signature.abstract_body || signature.modifiers.is_abstract)
                {
                    // The nearest declaration is the contract: an interface
                    // may redeclare its parent's method with more optional
                    // parameters (Doctrine's EntityManagerInterface::find()).
                    contract = contract.or(Some(signature));
                } else {
                    return Some(signature);
                }
            }
        }
        contract
    }

    pub fn complete(&self, class: &str) -> bool {
        self.ancestry(class).iter().all(|c| {
            c.parents
                .iter()
                .chain(&c.interfaces)
                .chain(&c.traits)
                .all(|name| self.class(name).is_some())
        })
    }

    pub fn property(&self, class: &str, property: &str) -> Option<&PropertyInfo> {
        self.ancestry(class)
            .into_iter()
            .find_map(|c| c.properties.get(property))
    }

    pub fn constant(&self, class: &str, constant: &str) -> Option<&ConstantInfo> {
        self.ancestry(class)
            .iter()
            .find_map(|c| c.constants.get(constant))
    }

    pub fn open_properties(&self, class: &str) -> bool {
        self.ancestry(class).iter().any(|c| {
            c.open_properties || c.methods.contains_key("__get") || c.methods.contains_key("__set")
        })
    }

    pub fn documented_method(&self, class: &str, method: &str, static_call: bool) -> bool {
        let method = method.to_ascii_lowercase();
        self.documents_member(class, |ancestor, origin| {
            let docs = &ancestor.documented;
            if static_call {
                docs.static_methods.contains(&method)
                    || (!origin
                        && ancestor
                            .methods
                            .get(&method)
                            .is_some_and(|method| method.modifiers.is_static))
            } else {
                docs.methods.contains(&method)
                    || docs.static_methods.contains(&method)
                    || (!origin && ancestor.methods.contains_key(&method))
            }
        })
    }

    pub fn documented_property(&self, class: &str, property: &str) -> bool {
        self.documents_member(class, |ancestor, origin| {
            ancestor.documented.properties.contains(property)
                || (!origin && ancestor.properties.contains_key(property))
        })
    }

    fn documents_member(&self, class: &str, has_member: impl Fn(&ClassInfo, bool) -> bool) -> bool {
        let mut pending = vec![(class, true)];
        let mut seen = BTreeSet::new();
        while let Some((class, origin)) = pending.pop() {
            for ancestor in self.ancestry(class) {
                if seen.insert((&ancestor.name, origin)) {
                    if has_member(ancestor, origin) {
                        return true;
                    }
                    pending.extend(
                        ancestor
                            .documented
                            .mixins
                            .iter()
                            .map(|name| (name.as_str(), false)),
                    );
                }
            }
        }
        false
    }

    pub fn public_visibility() -> Visibility {
        Visibility::Public
    }
}
