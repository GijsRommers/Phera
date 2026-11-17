//! Native and source declarations inherit the same runtime tables before objects are allocated.

use std::{rc::Rc, sync::Arc};

use phera_bytecode::{ClassId, ClassKind, CompiledUnit, Visibility};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object};

use super::class::PropertyDefault;
use super::{Class, Method, MethodBody};
use crate::Vm;

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
                signature: std::cell::OnceCell::new(),
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
                self.check_method_contract(inherited, &linked, &unit)?;
            }
            methods.insert(method_name.to_ascii_lowercase(), linked);
        }
        for interface in &interfaces {
            for (name, method) in &interface.methods {
                if let Some(implementation) = methods.get(name) {
                    self.check_method_contract(method, implementation, &unit)?;
                }
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
        let mut defaults = parent
            .as_ref()
            .map(|class| class.defaults.clone())
            .unwrap_or_default();
        let mut constants = parent
            .as_ref()
            .map(|class| class.constants.clone())
            .unwrap_or_default();
        for interface in &interfaces {
            constants.extend(interface.constants.clone());
        }
        for constant in &declaration.constants {
            let constant_name = unit.names[constant.name.index()].clone();
            if constants
                .get(&constant_name)
                .is_some_and(|constant| constant.is_final)
            {
                let declaring = constants[&constant_name].declaring.clone();
                return Err(NativeError::error(format!(
                    "{name}::{constant_name} cannot override final constant {declaring}::{constant_name}"
                )));
            }
            constants.insert(
                constant_name,
                Rc::new(super::LinkedConstant {
                    declaring: name.clone(),
                    visibility: constant.visibility,
                    is_final: constant.is_final,
                    initializer: crate::function_table::FunctionRef {
                        unit: unit.clone(),
                        index: constant.initializer.index(),
                    },
                    state: std::cell::RefCell::new(super::ConstantState::Uninitialized),
                    ty: constant.ty.map(|index| unit.types[index.index()].clone()),
                }),
            );
        }
        let mut static_properties = parent
            .as_ref()
            .map(|class| class.static_properties.clone())
            .unwrap_or_default();
        for property in &declaration.properties {
            let property_name = &unit.names[property.name.index()];
            let key = match property.visibility {
                Visibility::Public => ArrayKey::str(property_name.as_bytes()),
                Visibility::Protected => Object::protected_key(property_name),
                Visibility::Private => Object::private_key(name, property_name),
            };
            let value = match property.default {
                Some(phera_bytecode::Initializer::Literal(index)) => {
                    crate::frame::value(&unit.constants[index.index()])
                }
                _ => phera_value::Value::Null,
            };
            let value = if property.is_static {
                phera_value::Value::Reference(Rc::new(std::cell::RefCell::new(value)))
            } else {
                value
            };
            let mut linked = PropertyDefault {
                key: key.clone(),
                value,
                name: property_name.clone(),
                declaring: name.clone(),
                visibility: property.visibility,
                initialized: property.default.is_some() || property.ty.is_none(),
                initializer: match property.default {
                    Some(phera_bytecode::Initializer::Function(index)) => {
                        Some(crate::function_table::FunctionRef {
                            unit: unit.clone(),
                            index: index.index(),
                        })
                    }
                    _ => None,
                },
                ty: property.ty.map(|index| unit.types[index.index()].clone()),
                is_readonly: property.is_readonly,
                static_initialized: Rc::new(std::cell::Cell::new(false)),
                value_initialized: Rc::new(std::cell::Cell::new(
                    property.default.is_some() || property.ty.is_none(),
                )),
            };
            if let Some(value) = linked
                .initializer
                .as_ref()
                .and_then(super::class::literal_initializer)
            {
                if property.is_static {
                    if let phera_value::Value::Reference(cell) = &linked.value {
                        *cell.borrow_mut() = value;
                    }
                } else {
                    linked.value = value;
                }
                linked.initializer = None;
            }
            let target = if property.is_static {
                &mut static_properties
            } else {
                &mut defaults
            };
            if let Some(inherited) = target.iter_mut().find(|entry| entry.key == key) {
                *inherited = linked;
            } else {
                target.push(linked);
            }
        }
        let allows_dynamic_properties = declaration.allows_dynamic_properties
            || parent
                .as_ref()
                .is_some_and(|parent| parent.allows_dynamic_properties);
        self.classes.order.push(name.clone());
        self.classes.entries.insert(
            name.to_ascii_lowercase(),
            Rc::new_cyclic(|identity| Class {
                identity: identity.clone(),
                declaration_matches: std::cell::RefCell::new(std::collections::HashMap::new()),
                ancestors: super::class::ancestor_names(&parent, &interfaces),
                empty_constructor: super::class::empty_constructor(name),
                name_value: phera_value::Value::from(name.as_str()),
                name: name.clone(),
                parent,
                interfaces,
                kind: declaration.kind,
                is_abstract: declaration.is_abstract,
                is_final: declaration.is_final,
                is_readonly: declaration.is_readonly,
                allows_dynamic_properties,
                constructor: methods.get("__construct").cloned(),
                methods,
                property_template: super::class::property_template(&defaults),
                defaults,
                static_properties,
                constants,
            }),
        );
        Ok(())
    }
}
