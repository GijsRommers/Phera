//! Native classes enter the same root-first property layout and inherited method tables as PHP classes.

use super::class::PropertyDefault;
use super::{Class, ClassTable, Method, MethodBody};
use phera_bytecode::{ClassKind, Visibility};
use phera_extension_api::{ClassKind as NativeKind, NativeMethod};
use phera_value::{ArrayKey, Object};
use std::{collections::HashMap, rc::Rc};

impl ClassTable {
    /// Registries guarantee parents-first order, allowing native classes to use ordinary inheritance.
    pub(crate) fn native(
        registry: &phera_extension_api::Registry,
        signatures: &mut crate::signatures::Signatures,
    ) -> Self {
        let mut table = Self::default();
        table.order.push("stdClass".into());
        table.insert(
            "stdclass".into(),
            Rc::new_cyclic(|identity| Class {
                index: super::class_table::ClassIndex(0),
                ancestry: Vec::new(),
                origin: super::class::ClassOrigin::Native,
                identity: identity.clone(),
                declaration_matches: std::cell::RefCell::new(std::collections::HashMap::new()),
                ancestors: std::collections::HashSet::new(),
                empty_constructor: super::class::empty_constructor("stdClass", signatures),
                name_value: phera_value::Value::from("stdClass"),
                name: "stdClass".into(),
                parent: None,
                interfaces: Vec::new(),
                kind: ClassKind::Class,
                is_abstract: false,
                is_throwable: false,
                is_final: false,
                is_readonly: false,
                allows_dynamic_properties: true,
                methods: super::method_table::MethodTable::default(),
                constructor: None,
                property_template: Some(phera_value::PhpArray::new()),
                defaults: Vec::new(),
                static_properties: Vec::new(),
                constants: HashMap::new(),
            }),
        );
        let closure = phera_extension_api::ClassEntry {
            kind: NativeKind::FinalClass,
            ..phera_extension_api::ClassEntry::class("Closure")
        };
        for entry in registry.classes().chain(std::iter::once(&closure)) {
            let class_index = super::class_table::ClassIndex(table.entries.len() as u32);
            let parent = entry.parent.and_then(|name| table.get(name));
            let mut methods = parent
                .as_ref()
                .map(|class| class.methods.clone())
                .unwrap_or_default();
            for method in entry.methods {
                methods.insert(
                    table.names.intern(method.name),
                    method.name.to_ascii_lowercase(),
                    Rc::new(Method {
                        declaring_index: class_index,
                        signature: signatures.insert(
                            format!("{}::{}", entry.name, method.name),
                            method
                                .params
                                .iter()
                                .map(crate::calls::call_state::ParameterSpec::native)
                                .collect(),
                        ),
                        name: method.name.into(),
                        declaring: entry.name.into(),
                        visibility: native_visibility(method.visibility),
                        is_static: method.is_static,
                        is_final: method.is_final,
                        is_abstract: matches!(method.handler, NativeMethod::Abstract),
                        body: MethodBody::Native(*method),
                    }),
                );
            }
            if entry.name == "Closure" {
                for &(operation, name) in crate::closure_methods::ClosureMethod::entries() {
                    methods.insert(
                        table.names.intern(name),
                        name.to_ascii_lowercase(),
                        Rc::new(Method {
                            declaring_index: class_index,
                            signature: signatures.insert(
                                format!("Closure::{name}"),
                                operation
                                    .parameters()
                                    .iter()
                                    .map(crate::calls::call_state::ParameterSpec::native)
                                    .collect(),
                            ),
                            name: name.into(),
                            declaring: "Closure".into(),
                            visibility: Visibility::Public,
                            is_static: matches!(
                                operation,
                                crate::closure_methods::ClosureMethod::Bind
                                    | crate::closure_methods::ClosureMethod::FromCallable
                            ),
                            is_final: false,
                            is_abstract: false,
                            body: MethodBody::Closure(operation),
                        }),
                    );
                }
            }
            let mut defaults = parent
                .as_ref()
                .map(|class| class.defaults.clone())
                .unwrap_or_default();
            for property in entry
                .properties
                .iter()
                .filter(|property| !property.is_static)
            {
                let key = match native_visibility(property.visibility) {
                    Visibility::Private => Object::private_key(entry.name, property.name),
                    Visibility::Protected => Object::protected_key(property.name),
                    Visibility::Public => ArrayKey::str(property.name),
                };
                defaults.push(PropertyDefault {
                    key,
                    value: property.default.to_value(),
                    name: property.name.into(),
                    declaring: entry.name.into(),
                    visibility: native_visibility(property.visibility),
                    initialized: true,
                    initializer: None,
                    ty: None,
                    is_readonly: false,
                    static_initialized: Rc::new(std::cell::Cell::new(true)),
                    value_initialized: Rc::new(std::cell::Cell::new(true)),
                });
            }
            if table.get(entry.name).is_none() {
                table.order.push(entry.name.into());
            }
            let interfaces: Vec<Rc<Class>> = entry
                .interfaces
                .iter()
                .filter_map(|name| table.get(name))
                .collect();
            let is_throwable = entry.name == "Throwable"
                || parent.as_ref().is_some_and(|parent| parent.is_throwable)
                || interfaces
                    .iter()
                    .any(|interface: &Rc<Class>| interface.is_throwable);
            table.insert(
                entry.name.to_ascii_lowercase(),
                Rc::new_cyclic(|identity| Class {
                    index: class_index,
                    ancestry: super::class::ancestor_indexes(&parent, &interfaces),
                    origin: if entry.name == "Closure" {
                        super::class::ClassOrigin::Closure
                    } else {
                        super::class::ClassOrigin::Native
                    },
                    identity: identity.clone(),
                    declaration_matches: std::cell::RefCell::new(std::collections::HashMap::new()),
                    ancestors: super::class::ancestor_names(&parent, &interfaces),
                    empty_constructor: super::class::empty_constructor(entry.name, signatures),
                    name_value: phera_value::Value::from(entry.name),
                    name: entry.name.into(),
                    parent,
                    interfaces,
                    kind: if entry.kind == NativeKind::Interface {
                        ClassKind::Interface
                    } else {
                        ClassKind::Class
                    },
                    is_abstract: entry.kind == NativeKind::AbstractClass,
                    is_throwable,
                    is_final: entry.kind == NativeKind::FinalClass,
                    is_readonly: false,
                    allows_dynamic_properties: false,
                    constructor: methods.get("__construct").cloned(),
                    methods,
                    property_template: super::class::property_template(&defaults),
                    defaults,
                    static_properties: Vec::new(),
                    constants: HashMap::new(),
                }),
            );
        }
        table
    }
}

/// Preserve native visibility without coupling bytecode to extension metadata.
fn native_visibility(visibility: phera_extension_api::Visibility) -> Visibility {
    match visibility {
        phera_extension_api::Visibility::Public => Visibility::Public,
        phera_extension_api::Visibility::Protected => Visibility::Protected,
        phera_extension_api::Visibility::Private => Visibility::Private,
    }
}
