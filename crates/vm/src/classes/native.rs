//! Native classes enter the same root-first property layout and inherited method tables as PHP classes.

use super::class::PropertyDefault;
use super::{Class, ClassTable, Method, MethodBody};
use phera_bytecode::{ClassKind, Visibility};
use phera_extension_api::{ClassKind as NativeKind, NativeMethod};
use phera_value::{ArrayKey, Object};
use std::{collections::HashMap, rc::Rc};

impl ClassTable {
    /// Registries guarantee parents-first order, allowing native classes to use ordinary inheritance.
    pub(crate) fn native(registry: &phera_extension_api::Registry) -> Self {
        let mut table = Self::default();
        table.order.push("stdClass".into());
        table.entries.insert(
            "stdclass".into(),
            Rc::new(Class {
                name: "stdClass".into(),
                parent: None,
                interfaces: Vec::new(),
                kind: ClassKind::Class,
                is_abstract: false,
                is_final: false,
                is_readonly: false,
                allows_dynamic_properties: true,
                methods: HashMap::new(),
                defaults: Vec::new(),
                static_properties: Vec::new(),
                constants: HashMap::new(),
            }),
        );
        for entry in registry.classes() {
            let parent = entry.parent.and_then(|name| table.get(name));
            let mut methods = parent
                .as_ref()
                .map(|class| class.methods.clone())
                .unwrap_or_default();
            for method in entry.methods {
                methods.insert(
                    method.name.to_ascii_lowercase(),
                    Rc::new(Method {
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
            table.entries.insert(
                entry.name.to_ascii_lowercase(),
                Rc::new(Class {
                    name: entry.name.into(),
                    parent,
                    interfaces: entry
                        .interfaces
                        .iter()
                        .filter_map(|name| table.get(name))
                        .collect(),
                    kind: if entry.kind == NativeKind::Interface {
                        ClassKind::Interface
                    } else {
                        ClassKind::Class
                    },
                    is_abstract: entry.kind == NativeKind::AbstractClass,
                    is_final: entry.kind == NativeKind::FinalClass,
                    is_readonly: false,
                    allows_dynamic_properties: false,
                    methods,
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
