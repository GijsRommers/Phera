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
        for &index in &unit.hoisted_classes {
            let declaration = &unit.classes[index.index()];
            if declaration.interfaces.is_empty()
                && declaration.parent.is_none_or(|parent| {
                    self.classes
                        .get(&unit.symbols[parent.index()].written)
                        .is_some()
                })
                && self.classes.get(&declaration.name).is_none()
            {
                if let Err(error) = self.link_class(unit.clone(), index) {
                    if crate::autoload::ClassMiss::is(&error) {
                        continue;
                    }
                    return Err(error);
                }
                self.prelinked_classes
                    .insert((Arc::as_ptr(unit) as usize, index.index()));
            }
        }
        Ok(())
    }

    /// Runtime declarations are not idempotent; only the exact prelinked instruction is skipped.
    pub(crate) fn declare_class(&mut self, index: ClassId) -> Result<(), NativeError> {
        let unit = self
            .current()
            .ok_or_else(|| internal_error!("class frame"))?
            .unit()
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
        self.link_class_body(unit.clone(), index)
            .map_err(|error| super::declaration_errors::link_failure(&unit, index, error))
    }

    /// Structural declaration failures are fatal; missing dependencies remain catchable Errors.
    fn link_class_body(
        &mut self,
        unit: Arc<CompiledUnit>,
        declaration_index: ClassId,
    ) -> Result<(), NativeError> {
        let declaration = &unit.classes[declaration_index.index()];
        let name = &declaration.name;
        if let Some(existing) = self.classes.get(name) {
            return Err(NativeError::error(format!(
                "Cannot redeclare class {}",
                existing.name
            )));
        }
        let parent = declaration
            .parent
            .map(|symbol| {
                let parent_name = &unit.symbols[symbol.index()].written;
                self.classes
                    .get(parent_name)
                    .ok_or_else(|| Self::class_miss(parent_name, "Class"))
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
        let mut interfaces = declaration
            .interfaces
            .iter()
            .map(|symbol| {
                let interface_name = &unit.symbols[symbol.index()].written;
                let interface = self
                    .classes
                    .get(interface_name)
                    .ok_or_else(|| Self::class_miss(interface_name, "Interface"))?;
                if interface.kind != ClassKind::Interface {
                    return Err(NativeError::error(format!(
                        "{name} cannot implement {interface_name} - it is not an interface"
                    )));
                }
                Ok(interface)
            })
            .collect::<Result<Vec<_>, NativeError>>()?;
        let mut seen = std::collections::HashSet::new();
        if let Some(duplicate) = interfaces
            .iter()
            .find(|interface| !seen.insert(interface.name.as_str()))
        {
            let kind = if declaration.kind == ClassKind::Interface {
                "Interface"
            } else {
                "Class"
            };
            return Err(NativeError::error(format!(
                "{kind} {name} cannot implement previously implemented interface {}",
                duplicate.name
            )));
        }
        if declaration.kind != ClassKind::Interface
            && interfaces
                .iter()
                .any(|interface| interface.is_a("Throwable"))
            && parent
                .as_ref()
                .is_none_or(|parent| !parent.is_a("Throwable"))
        {
            return Err(NativeError::error(format!(
                "Class {name} cannot implement interface Throwable, extend Exception or \
                 Error instead"
            )));
        }
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
                body: MethodBody::User(self.code.index_in(&unit, Some(method.function.index()))),
            });
            if let Some(inherited) = methods.get(&method_name.to_ascii_lowercase()) {
                super::inheritance_checks::check_override(name, inherited, &linked)
                    .map_err(|error| self.magic_override_error(&linked, error))?;
                self.check_method_contract(inherited, &linked, &unit)?;
            }
            methods.insert(method_name.to_ascii_lowercase(), linked);
        }
        if let Some(method) = methods.get("__tostring") {
            if method.visibility != Visibility::Public
                && let MethodBody::User(function) = method.body
            {
                let file = self.code.unit(function).file.clone();
                let line = self.code.function(function).definition_line as usize;
                let message = format!(
                    "Access level to {}::{}() must be public (as in class Stringable)",
                    method.declaring, method.name
                );
                return Err(NativeError::Unwind(phera_extension_api::Unwind(Box::new(
                    crate::error_traces::DeclarationFailure {
                        message,
                        file,
                        line,
                    },
                ))));
            }
            if !interfaces
                .iter()
                .any(|interface| interface.is_a("Stringable"))
                && !parent
                    .as_ref()
                    .is_some_and(|parent| parent.is_a("Stringable"))
                && let Some(stringable) = self.classes.get("Stringable")
            {
                interfaces.push(stringable);
            }
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
        super::declaration_errors::merge_interface_constants(
            declaration,
            &unit,
            &interfaces,
            &mut constants,
        )?;
        for constant in &declaration.constants {
            let constant_name = unit.names[constant.name.index()].clone();
            if let Some(inherited) = constants.get(&constant_name) {
                let narrower = match inherited.visibility {
                    Visibility::Public => constant.visibility != Visibility::Public,
                    Visibility::Protected => constant.visibility == Visibility::Private,
                    Visibility::Private => false,
                };
                if narrower {
                    let visibility = match inherited.visibility {
                        Visibility::Public => "public",
                        Visibility::Protected => "protected",
                        Visibility::Private => unreachable!(),
                    };
                    return Err(NativeError::error(format!(
                        "Access level to {name}::{constant_name} must be {visibility} \
                         (as in class {})",
                        inherited.declaring
                    )));
                }
            }
            if constants
                .get(&constant_name)
                .is_some_and(|constant| constant.is_final)
            {
                let declaring = constants[&constant_name].declaring.clone();
                return Err(NativeError::error(format!(
                    "{name}::{constant_name} cannot override final constant \
                     {declaring}::{constant_name}"
                )));
            }
            if let Some(inherited) = constants.get(&constant_name)
                && inherited.visibility != Visibility::Private
                && let Some(expected) = &inherited.ty
            {
                let declared = constant.ty.map(|index| &unit.types[index.index()]);
                let compatible = self.type_subset(
                    declared.map(|declared| declared.display.as_str()),
                    name,
                    Some(&expected.display),
                    &inherited.declaring,
                    &unit,
                );
                if !compatible {
                    return Err(NativeError::error(format!(
                        "Type of {name}::{constant_name} must be compatible with \
                         {}::{constant_name} of type {}",
                        inherited.declaring, expected.display
                    )));
                }
            }
            constants.insert(
                constant_name,
                Rc::new(super::LinkedConstant {
                    deprecated: None,
                    declaring: name.clone(),
                    visibility: constant.visibility,
                    is_final: constant.is_final,
                    initializer: super::class::ConstantInitializer::Compiled {
                        function: self
                            .code
                            .index_in(&unit, Some(constant.initializer.index())),
                        state: std::cell::RefCell::new(super::ConstantState::Uninitialized),
                    },
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
                    crate::unit_caches::value(&unit.constants[index.index()])
                }
                _ => phera_value::Value::Null,
            };
            let mut linked = PropertyDefault {
                key: key.clone(),
                static_storage: property
                    .is_static
                    .then(|| crate::reference_holders::StaticProperty::new(value.clone())),
                value,
                name: property_name.clone(),
                declaring: name.clone(),
                visibility: property.visibility,
                initialized: property.default.is_some() || property.ty.is_none(),
                initializer: match property.default {
                    Some(phera_bytecode::Initializer::Function(index)) => {
                        Some(self.code.index_in(&unit, Some(index.index())))
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
                .and_then(|reference| super::class::literal_initializer(&self.code, reference))
            {
                if let Some(storage) = &linked.static_storage {
                    *storage.reference.borrow().borrow_mut() = value.clone();
                }
                linked.value = value;
                linked.initializer = None;
            }
            let inherited = if property.is_static {
                &static_properties
            } else {
                &defaults
            };
            super::declaration_errors::check_property_visibility(
                name,
                property,
                property_name,
                inherited,
            )?;
            let target = if property.is_static {
                &mut static_properties
            } else {
                &mut defaults
            };
            if let Some(inherited) = target.iter_mut().find(|entry| {
                entry.key == key
                    || (entry.name == *property_name
                        && entry.visibility != Visibility::Private
                        && property.visibility != Visibility::Private)
            }) {
                *inherited = linked;
            } else {
                target.push(linked);
            }
        }
        let allows_dynamic_properties = declaration.allows_dynamic_properties
            || parent
                .as_ref()
                .is_some_and(|parent| parent.allows_dynamic_properties);
        let native_clone = parent
            .as_ref()
            .map_or(phera_extension_api::NativeClone::Properties, |parent| {
                parent.native_clone
            });
        let property_access = parent.as_ref().and_then(|parent| parent.property_access);
        let is_throwable = parent.as_ref().is_some_and(|parent| parent.is_throwable)
            || interfaces.iter().any(|interface| interface.is_throwable);
        self.classes.order.push(name.clone());
        let index = self.classes.next_index();
        for method in &declaration.methods {
            let function = self.code.index_in(&unit, Some(method.function.index()));
            self.code.set_declaring_class(function, index);
        }
        let declaration_location = Some((self.code.index_in(&unit, None), declaration.line));
        self.classes.insert(
            name.to_ascii_lowercase(),
            Rc::new_cyclic(|identity| Class {
                index,
                origin: super::class::ClassOrigin::User,
                identity: identity.clone(),
                declaration_matches: std::cell::RefCell::new(std::collections::HashMap::new()),
                ancestors: super::class::ancestor_names(&parent, &interfaces),
                name_value: phera_value::Value::from(name.as_str()),
                name: name.clone(),
                parent,
                interfaces,
                kind: declaration.kind,
                is_abstract: declaration.is_abstract,
                is_throwable,
                is_final: declaration.is_final,
                is_readonly: declaration.is_readonly,
                allows_dynamic_properties,
                property_access,
                native_clone,
                constructor: methods.get("__construct").cloned(),
                magic: super::magic::MagicMethods::link(&methods),
                methods,
                method_table: std::cell::OnceCell::new(),
                property_template: super::class::property_template(&defaults),
                defaults,
                static_properties,
                constants,
                declaration: declaration_location,
            }),
        );
        Ok(())
    }
}
