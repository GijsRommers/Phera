//! Native classes enter the same root-first property layout and inherited method tables as PHP classes.

use super::class::PropertyDefault;
use super::{Class, ClassTable, Method, MethodBody};
use phera_bytecode::{ClassKind, Type, TypeMask, Visibility};
use phera_extension_api::{ClassKind as NativeKind, ConstantValue, NativeMethod};
use phera_value::{ArrayKey, Object};
use std::{collections::HashMap, rc::Rc};

impl ClassTable {
    /// Registries guarantee parents-first order, allowing native classes to use ordinary inheritance.
    pub(crate) fn native(registry: &phera_extension_api::Registry) -> Self {
        let mut table = Self::default();
        table.order.push("stdClass".into());
        let index = table.next_index();
        table.insert(
            "stdclass".into(),
            Rc::new_cyclic(|identity| Class {
                index,
                hooks: None,
                property_access: None,
                operations: super::operations::ClassOperations::default(),
                origin: super::class::ClassOrigin::Native,
                identity: identity.clone(),
                declaration_matches: std::cell::RefCell::new(std::collections::HashMap::new()),
                ancestors: std::collections::HashSet::new(),
                name_value: phera_value::Value::from("stdClass"),
                name: "stdClass".into(),
                parent: None,
                interfaces: Vec::new(),
                traits: Vec::new(),
                enum_info: None,
                kind: ClassKind::Class,
                is_abstract: false,
                is_throwable: false,
                has_native_layout: true,
                is_final: false,
                is_readonly: false,
                allows_dynamic_properties: true,
                methods: HashMap::new(),
                method_table: std::cell::OnceCell::new(),
                serialized_fields: std::cell::OnceCell::new(),
                constructor: None,
                magic: None,
                property_template: Some(phera_value::PhpArray::new()),
                defaults: Vec::new(),
                static_properties: Vec::new(),
                constants: HashMap::new(),
                declaration: None,
            }),
        );
        let closure = phera_extension_api::ClassEntry {
            kind: NativeKind::FinalClass,
            ..phera_extension_api::ClassEntry::class("Closure")
        };
        let internal_iterator = registry
            .get_class("Iterator")
            .map(|_| &crate::internal_iterator::ENTRY);
        for entry in registry
            .classes()
            .chain(std::iter::once(&closure))
            .chain(internal_iterator)
            .chain(
                registry
                    .get_class("Iterator")
                    .map(|_| &crate::generators::class::ENTRY),
            )
            .chain(
                registry
                    .get_class("Exception")
                    .map(|_| &crate::generators::class::CLOSED_EXCEPTION),
            )
            .chain(super::enums::INTERFACES)
        {
            let parent = entry.parent.and_then(|name| table.get(name));
            let mut methods = parent
                .as_ref()
                .map(|class| class.methods.clone())
                .unwrap_or_default();
            for method in entry.methods {
                methods.insert(
                    method.name.to_ascii_lowercase(),
                    Rc::new(Method {
                        signature: std::cell::OnceCell::new(),
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
                        name.to_ascii_lowercase(),
                        Rc::new(Method {
                            signature: std::cell::OnceCell::new(),
                            name: name.into(),
                            declaring: "Closure".into(),
                            visibility: if operation
                                == crate::closure_methods::ClosureMethod::Construct
                            {
                                Visibility::Private
                            } else {
                                Visibility::Public
                            },
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
                    set_visibility: None,
                    is_final: false,
                    static_storage: None,
                    key,
                    value: property
                        .default
                        .map(|value| value.to_value())
                        .unwrap_or_default(),
                    name: property.name.into(),
                    declaring: entry.name.into(),
                    visibility: native_visibility(property.visibility),
                    initialized: property.default.is_some(),
                    initializer: None,
                    ty: property.ty.map(property_type),
                    is_readonly: property.is_readonly,
                    static_initialized: Rc::new(std::cell::Cell::new(true)),
                    value_initialized: Rc::new(std::cell::Cell::new(property.default.is_some())),
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
            if entry.kind == NativeKind::Interface {
                for interface in &interfaces {
                    for (name, method) in &interface.methods {
                        methods
                            .entry(name.clone())
                            .or_insert_with(|| method.clone());
                    }
                }
            }
            let mut constants = parent
                .as_ref()
                .map(|class| class.constants.clone())
                .unwrap_or_default();
            for interface in &interfaces {
                constants.extend(interface.constants.clone());
            }
            for (name, value) in entry.constants {
                constants.insert(
                    (*name).into(),
                    Rc::new(super::LinkedConstant {
                        deprecated: entry
                            .deprecated_constants
                            .iter()
                            .find(|(key, _)| key == name)
                            .map(|(_, message)| (*message).to_owned()),
                        declaring: entry.name.into(),
                        visibility: Visibility::Public,
                        is_final: false,
                        initializer: super::class::ConstantInitializer::Value(value.to_value()),
                        ty: Some(constant_type(value)),
                    }),
                );
            }
            let is_throwable = entry.name == "Throwable"
                || parent.as_ref().is_some_and(|parent| parent.is_throwable)
                || interfaces
                    .iter()
                    .any(|interface: &Rc<Class>| interface.is_throwable);
            let hooks = super::operations::inherit_hooks(entry.hooks, parent.as_deref());
            let operations = super::operations::ClassOperations::link(
                entry.name,
                parent.as_deref(),
                &interfaces,
            )
            .with_native_dimensions(hooks.as_deref(), &methods);
            let index = table.next_index();
            table.insert(
                entry.name.to_ascii_lowercase(),
                Rc::new_cyclic(|identity| Class {
                    index,
                    property_access: hooks.as_ref().and_then(|hooks| hooks.property_access),
                    hooks,
                    operations,
                    origin: if entry.name == "Closure" {
                        super::class::ClassOrigin::Closure
                    } else {
                        super::class::ClassOrigin::Native
                    },
                    identity: identity.clone(),
                    declaration_matches: std::cell::RefCell::new(std::collections::HashMap::new()),
                    ancestors: super::class::ancestor_names(&parent, &interfaces),
                    name_value: phera_value::Value::from(entry.name),
                    name: entry.name.into(),
                    parent,
                    interfaces,
                    traits: Vec::new(),
                    enum_info: None,
                    kind: if entry.kind == NativeKind::Interface {
                        ClassKind::Interface
                    } else {
                        ClassKind::Class
                    },
                    is_abstract: entry.kind == NativeKind::AbstractClass,
                    is_throwable,
                    has_native_layout: true,
                    is_final: entry.kind == NativeKind::FinalClass,
                    is_readonly: false,
                    allows_dynamic_properties: false,
                    constructor: methods.get("__construct").cloned(),
                    magic: super::magic::MagicMethods::link(&methods),
                    methods,
                    method_table: std::cell::OnceCell::new(),
                    serialized_fields: std::cell::OnceCell::new(),
                    property_template: super::class::property_template(&defaults),
                    defaults,
                    static_properties: Vec::new(),
                    constants,
                    declaration: None,
                }),
            );
        }
        table
    }
}

/// PHP's native constants carry scalar types, so `const int FETCH_ASSOC = 9` is a valid override.
fn constant_type(value: &ConstantValue) -> Type {
    let (mask, display) = match value {
        ConstantValue::Null => (TypeMask::NULL, "null"),
        ConstantValue::Bool(_) => (TypeMask::BOOL, "bool"),
        ConstantValue::Int(_) => (TypeMask::INT, "int"),
        ConstantValue::Float(_) => (TypeMask::FLOAT, "float"),
        ConstantValue::String(_) => (TypeMask::STRING, "string"),
        ConstantValue::EmptyArray => (TypeMask::ARRAY, "array"),
    };
    Type {
        mask,
        display: display.into(),
        classes: Vec::new(),
        intersection: false,
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

/// Native property types use the same storage and assignment checks as PHP declarations.
fn property_type(ty: phera_extension_api::PropertyType) -> Type {
    use phera_extension_api::PropertyType;
    let (mask, name) = match ty {
        PropertyType::Bool => (TypeMask::BOOL, "bool"),
        PropertyType::Int => (TypeMask::INT, "int"),
        PropertyType::Float => (TypeMask::FLOAT, "float"),
        PropertyType::String => (TypeMask::STRING, "string"),
        PropertyType::NullableString => (TypeMask::STRING.union(TypeMask::NULL), "?string"),
        PropertyType::Array => (TypeMask::ARRAY, "array"),
        PropertyType::Object => (TypeMask::OBJECT, "object"),
        PropertyType::Mixed => (TypeMask::MIXED, "mixed"),
        PropertyType::NullableClass(name) => {
            return Type {
                mask: TypeMask::NULL,
                display: format!("?{name}"),
                classes: Vec::new(),
                intersection: false,
            };
        }
    };
    Type {
        mask,
        display: name.into(),
        classes: Vec::new(),
        intersection: false,
    }
}
