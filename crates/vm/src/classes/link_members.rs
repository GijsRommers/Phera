//! Class member declarations keep lazy defaults: \`class C { public int $n = self::N; }\`.

use std::{collections::HashMap, rc::Rc};

use phera_bytecode::{ClassDeclaration, CompiledUnit, Visibility};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object};

use super::{Class, LinkedConstant, class::PropertyDefault};
use crate::{Vm, code_table::FunctionIndex};

impl Vm {
    pub(super) fn link_class_constants(
        &self,
        declaration: &ClassDeclaration,
        unit: &CompiledUnit,
        unit_first: FunctionIndex,
        parent: Option<&Rc<Class>>,
        interfaces: &[Rc<Class>],
    ) -> Result<HashMap<String, Rc<LinkedConstant>>, NativeError> {
        let name = &declaration.name;
        let mut constants = parent
            .map(|class| class.constants.clone())
            .unwrap_or_default();
        super::declaration_errors::merge_interface_constants(
            declaration,
            unit,
            interfaces,
            &mut constants,
        )?;
        for constant in &declaration.constants {
            let constant_name = unit.names[constant.name.index()].clone();
            if let Some(inherited) = constants.get(&constant_name) {
                let required = match inherited.visibility {
                    Visibility::Public if constant.visibility != Visibility::Public => {
                        Some("public")
                    }
                    Visibility::Protected if constant.visibility == Visibility::Private => {
                        Some("protected")
                    }
                    _ => None,
                };
                if let Some(visibility) = required {
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
                    unit,
                );
                if !compatible {
                    return Err(NativeError::error(format!(
                        "Type of {name}::{constant_name} must be compatible with \
                         {}::{constant_name} of type {}",
                        inherited.declaring, expected.display
                    )));
                }
            }
            let deprecated = if unit.has_link_attributes {
                crate::attribute_diagnostics::attribute(&constant.metadata.attributes, "Deprecated")
                    .map(|attribute| {
                        crate::attribute_diagnostics::deprecation_message(
                            attribute,
                            &format!("Constant {name}::{constant_name}"),
                        )
                    })
            } else {
                None
            };
            constants.insert(
                constant_name,
                Rc::new(LinkedConstant {
                    deprecated,
                    declaring: name.clone(),
                    visibility: constant.visibility,
                    is_final: constant.is_final,
                    initializer: super::class::ConstantInitializer::Compiled {
                        function: self.code.declared(unit_first, constant.initializer.index()),
                        state: std::cell::RefCell::new(super::ConstantState::Uninitialized),
                    },
                    ty: constant.ty.map(|index| unit.types[index.index()].clone()),
                }),
            );
        }
        Ok(constants)
    }

    pub(super) fn link_class_properties(
        &mut self,
        declaration: &ClassDeclaration,
        unit: &CompiledUnit,
        unit_first: FunctionIndex,
        parent: Option<&Rc<Class>>,
        interfaces: &[Rc<Class>],
    ) -> Result<(Vec<PropertyDefault>, Vec<PropertyDefault>), NativeError> {
        let name = &declaration.name;
        let mut defaults = parent
            .as_ref()
            .map(|class| class.defaults.clone())
            .unwrap_or_default();
        let mut static_properties = parent
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
            let hooks = property.hooks.as_ref().map(|hooks| {
                Rc::new(super::property_hooks::LinkedPropertyHooks::link(
                    hooks, name, self, unit_first,
                ))
            });
            let initialized = property.default.is_some()
                || (property.ty.is_none()
                    && hooks.as_ref().is_none_or(|hooks| !hooks.hooks.is_virtual));
            let mut linked = PropertyDefault {
                hooks,
                is_final: property.is_final,
                set_visibility: property.set_visibility,
                key: key.clone(),
                static_storage: property
                    .is_static
                    .then(|| crate::reference_holders::StaticProperty::new(value.clone())),
                value,
                name: property_name.clone(),
                declaring: name.clone(),
                visibility: property.visibility,
                initialized,
                initializer: match property.default {
                    Some(phera_bytecode::Initializer::Function(index)) => {
                        Some(self.code.declared(unit_first, index.index()))
                    }
                    _ => None,
                },
                ty: property.ty.map(|index| unit.types[index.index()].clone()),
                is_readonly: property.is_readonly,
                static_initialized: Rc::new(std::cell::Cell::new(false)),
                value_initialized: Rc::new(std::cell::Cell::new(initialized)),
            };
            self.verify_set_hook_type(&linked, unit, declaration.line as usize)?;
            if let Some(value) = linked
                .initializer
                .as_ref()
                .and_then(|reference| super::class::literal_initializer(&self.code, reference))
            {
                let value = super::class::widen_property_value(value, linked.ty.as_ref());
                if property.is_static {
                    if let Some(storage) = &linked.static_storage {
                        *storage.reference.borrow().borrow_mut() = value.clone();
                        linked.value = value;
                    }
                } else {
                    linked.value = value;
                }
                linked.initializer = None;
            }
            let inherited = if property.is_static {
                &static_properties
            } else {
                &defaults
            };
            if declaration.kind != phera_bytecode::ClassKind::Trait
                && unit.has_link_attributes
                && crate::attribute_diagnostics::attribute(
                    &property.metadata.attributes,
                    "Override",
                )
                .is_some()
                && !overrides_property(property_name, name, inherited, interfaces)
            {
                return Err(property_override_error(
                    declaration,
                    unit,
                    property_name,
                    property.metadata.span.start,
                ));
            }
            let other_storage = if property.is_static {
                &defaults
            } else {
                &static_properties
            };
            if parent.is_some()
                && let Some(inherited) = inherited.iter().chain(other_storage).find(|candidate| {
                    candidate.name == *property_name && candidate.visibility != Visibility::Private
                })
            {
                self.check_property_override(&linked, inherited, unit)?;
            }
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
                self.inherit_property_hooks(&mut linked, inherited, unit)?;
                *inherited = linked;
            } else {
                target.push(linked);
            }
        }
        Ok((defaults, static_properties))
    }
}

pub(super) fn overrides_property(
    name: &str,
    child: &str,
    properties: &[PropertyDefault],
    interfaces: &[Rc<Class>],
) -> bool {
    properties
        .iter()
        .chain(interfaces.iter().flat_map(|interface| &interface.defaults))
        .any(|candidate| {
            candidate.name == name
                && candidate.declaring != child
                && candidate.visibility != Visibility::Private
        })
}

pub(super) fn property_override_error(
    class: &ClassDeclaration,
    unit: &CompiledUnit,
    property_name: &str,
    offset: usize,
) -> NativeError {
    let name = phera_value::class_display_name(&class.name);
    NativeError::Unwind(phera_extension_api::Unwind(Box::new(
        crate::error_traces::DeclarationFailure {
            message: format!(
                r"{name}::${property_name} has #[\Override] attribute, but no matching parent property exists"
            ),
            file: unit.file.clone(),
            line: unit
                .reflection_lines
                .as_ref()
                .map_or(class.line as usize, |lines| lines.line(offset)),
        },
    )))
}
