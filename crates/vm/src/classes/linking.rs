//! Native and source declarations inherit the same runtime tables before objects are allocated.

use std::{collections::HashMap, rc::Rc, sync::Arc};

use phera_bytecode::{ClassId, ClassKind, CompiledUnit, Visibility};
use phera_extension_api::{ClassKind as NativeKind, NativeError, NativeMethod};
use phera_value::{ArrayKey, Object};

use super::class::PropertyDefault;
use super::{Class, ClassTable, Method, MethodBody};
use crate::Vm;

impl ClassTable {
    /// Registries guarantee parents-first order, allowing native classes to use ordinary inheritance.
    pub(crate) fn native(registry: &phera_extension_api::Registry) -> Self {
        let mut table = Self::default();
        table.entries.insert(
            "stdclass".into(),
            Rc::new(Class {
                name: "stdClass".into(),
                parent: None,
                interfaces: Vec::new(),
                kind: ClassKind::Class,
                is_abstract: false,
                is_final: false,
                methods: HashMap::new(),
                defaults: Vec::new(),
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
                });
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
                    methods,
                    defaults,
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

impl Vm {
    /// Hoist only declarations whose dependencies are available; deferred ones execute normally.
    pub(crate) fn hoist_classes(&mut self, unit: &Arc<CompiledUnit>) -> Result<(), NativeError> {
        let mut pending = unit.hoisted_classes.clone();
        loop {
            let before = pending.len();
            let mut deferred = Vec::new();
            for index in pending {
                let declaration = &unit.classes[index.index()];
                if declaration
                    .parent
                    .iter()
                    .chain(&declaration.interfaces)
                    .all(|symbol| {
                        self.classes
                            .get(&unit.symbols[symbol.index()].name)
                            .is_some()
                    })
                {
                    self.link_class(unit.clone(), index)?;
                    self.prelinked_classes
                        .insert((Arc::as_ptr(unit) as usize, index.index()));
                } else {
                    deferred.push(index);
                }
            }
            if deferred.len() == before || deferred.is_empty() {
                return Ok(());
            }
            pending = deferred;
        }
    }

    /// Runtime declarations are not idempotent; only the exact prelinked instruction is skipped.
    pub(crate) fn declare_class(&mut self, index: ClassId) -> Result<(), NativeError> {
        let unit = self
            .current()
            .ok_or_else(|| internal_error!("class frame"))?
            .unit
            .clone();
        if self
            .prelinked_classes
            .remove(&(Arc::as_ptr(&unit) as usize, index.index()))
        {
            return Ok(());
        }
        self.link_class(unit, index)
    }

    /// Copy inherited tables and validate structural dependencies before publishing a class.
    fn link_class(&mut self, unit: Arc<CompiledUnit>, index: ClassId) -> Result<(), NativeError> {
        let declaration = &unit.classes[index.index()];
        let name = &declaration.name;
        if self.classes.get(name).is_some() {
            return Err(NativeError::error(format!("Cannot redeclare class {name}")));
        }
        let parent = declaration
            .parent
            .map(|symbol| {
                let parent_name = &unit.symbols[symbol.index()].written;
                self.classes
                    .get(parent_name)
                    .ok_or_else(|| NativeError::error(format!("Class \"{parent_name}\" not found")))
            })
            .transpose()?;
        if let Some(parent) = &parent {
            if parent.is_final {
                return Err(NativeError::error(format!(
                    "Class {name} cannot extend final class {}",
                    parent.name
                )));
            }
            if parent.kind == ClassKind::Interface {
                return Err(NativeError::error(format!(
                    "Class {name} cannot extend interface {}",
                    parent.name
                )));
            }
        }
        let interfaces = declaration
            .interfaces
            .iter()
            .map(|symbol| {
                let interface_name = &unit.symbols[symbol.index()].written;
                let interface = self.classes.get(interface_name).ok_or_else(|| {
                    NativeError::error(format!("Interface \"{interface_name}\" not found"))
                })?;
                if interface.kind != ClassKind::Interface {
                    return Err(NativeError::error(format!(
                        "{name} cannot implement {interface_name} - it is not an interface"
                    )));
                }
                Ok(interface)
            })
            .collect::<Result<Vec<_>, NativeError>>()?;
        let mut methods = parent
            .as_ref()
            .map(|class| class.methods.clone())
            .unwrap_or_default();
        for method in &declaration.methods {
            let method_name = &unit.names[method.name.index()];
            let linked = Rc::new(Method {
                name: method_name.clone(),
                declaring: name.clone(),
                visibility: method.visibility,
                is_static: method.is_static,
                is_final: method.is_final,
                is_abstract: method.is_abstract,
                body: MethodBody::User(crate::function_table::FunctionRef {
                    unit: unit.clone(),
                    index: method.function.index(),
                }),
            });
            if let Some(inherited) = methods.get(&method_name.to_ascii_lowercase()) {
                super::inheritance_checks::check_override(name, inherited, &linked)?;
            }
            methods.insert(method_name.to_ascii_lowercase(), linked);
        }
        for interface in &interfaces {
            for (name, method) in &interface.methods {
                methods
                    .entry(name.clone())
                    .or_insert_with(|| method.clone());
            }
        }
        super::inheritance_checks::check_abstract(
            name,
            declaration.kind,
            declaration.is_abstract,
            &methods,
            &interfaces,
        )?;
        let defaults = parent
            .as_ref()
            .map(|class| class.defaults.clone())
            .unwrap_or_default();
        self.classes.entries.insert(
            name.to_ascii_lowercase(),
            Rc::new(Class {
                name: name.clone(),
                parent,
                interfaces,
                kind: declaration.kind,
                is_abstract: declaration.is_abstract,
                is_final: declaration.is_final,
                methods,
                defaults,
            }),
        );
        Ok(())
    }
}
