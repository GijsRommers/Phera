//! Class member declarations keep lazy defaults: \`class C { public int $n = self::N; }\`.

use std::{collections::HashMap, rc::Rc, sync::Arc};

use phera_bytecode::{ClassDeclaration, CompiledUnit, Visibility};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object};

use super::{Class, LinkedConstant, class::PropertyDefault};
use crate::Vm;

impl Vm {
    pub(super) fn link_class_constants(
        &self,
        declaration: &ClassDeclaration,
        unit: &Arc<CompiledUnit>,
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
            constants.insert(
                constant_name,
                Rc::new(LinkedConstant {
                    declaring: name.clone(),
                    visibility: constant.visibility,
                    is_final: constant.is_final,
                    initializer: super::class::ConstantInitializer::Compiled {
                        function: self.code.index_in(unit, Some(constant.initializer.index())),
                        state: std::cell::RefCell::new(super::ConstantState::Uninitialized),
                    },
                    ty: constant.ty.map(|index| unit.types[index.index()].clone()),
                }),
            );
        }
        Ok(constants)
    }

    pub(super) fn link_class_properties(
        &self,
        declaration: &ClassDeclaration,
        unit: &Arc<CompiledUnit>,
        parent: Option<&Rc<Class>>,
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
                        Some(self.code.index_in(unit, Some(index.index())))
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
            if let Some(inherited) = target.iter_mut().find(|entry| entry.key == key) {
                *inherited = linked;
            } else {
                target.push(linked);
            }
        }
        Ok((defaults, static_properties))
    }
}
