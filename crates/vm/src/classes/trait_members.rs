//! A trait's declared properties and constants belong to each consuming class.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

use phera_bytecode::{ClassDeclaration, CompiledUnit, Visibility};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object};

use super::{
    Class, ConstantState, LinkedConstant,
    class::{ConstantInitializer, PropertyDefault},
    trait_methods::display,
};
use crate::Vm;

impl Vm {
    pub(super) fn compose_trait_members(
        &mut self,
        declaration: &ClassDeclaration,
        unit: &CompiledUnit,
        traits: &[Rc<Class>],
        defaults: &mut Vec<PropertyDefault>,
        statics: &mut Vec<PropertyDefault>,
        constants: &mut HashMap<String, Rc<LinkedConstant>>,
    ) -> Result<(), NativeError> {
        let class = &declaration.name;
        let mut property_origins = HashMap::new();
        let mut constant_origins = HashMap::new();
        for provider in traits {
            let mut trait_constants: Vec<_> = provider.constants.iter().collect();
            trait_constants.sort_by_key(|(name, _)| *name);
            for (name, constant) in trait_constants {
                if declaration
                    .enum_cases
                    .iter()
                    .any(|case| unit.names[case.name.index()] == *name)
                {
                    return Err(NativeError::error(format!(
                        "Cannot use trait {}, because {}::{name} conflicts with enum case {class}::{name}",
                        provider.name, provider.name,
                    )));
                }
                if let Some(existing) = constants.get(name)
                    && existing.declaring == *class
                {
                    let agree = self.with_trait_constants(class, constants, |vm| {
                        vm.trait_constants_agree(existing, constant, class, unit)
                    })?;
                    if !agree {
                        let origin = constant_origins
                            .get(name.as_str())
                            .copied()
                            .unwrap_or(class);
                        return Err(incompatible(
                            "constant",
                            name,
                            class,
                            origin,
                            &provider.name,
                        ));
                    }
                    continue;
                }
                if let Some(inherited) = constants.get(name)
                    && inherited.is_final
                {
                    return Err(NativeError::error(format!(
                        "{class}::{name} cannot override final constant {}::{name}",
                        inherited.declaring,
                    )));
                }
                let initializer = match &constant.initializer {
                    ConstantInitializer::Value(value) => ConstantInitializer::Value(value.clone()),
                    ConstantInitializer::EnumCase { .. } => {
                        return Err(internal_error!("enum case in trait"));
                    }
                    ConstantInitializer::Compiled { function, .. } => {
                        ConstantInitializer::Compiled {
                            function: *function,
                            state: RefCell::new(ConstantState::Uninitialized),
                        }
                    }
                };
                constants.insert(
                    name.clone(),
                    Rc::new(LinkedConstant {
                        deprecated: None,
                        declaring: class.clone(),
                        visibility: constant.visibility,
                        is_final: constant.is_final,
                        initializer,
                        ty: constant.ty.clone(),
                    }),
                );
                constant_origins.insert(name.as_str(), &provider.name);
            }
        }
        for provider in traits {
            for (source, opposite) in [
                (&provider.defaults, &*statics),
                (&provider.static_properties, &*defaults),
            ] {
                for property in source {
                    let name = property.name.as_str();
                    if let Some(existing) = opposite.iter().find(|existing| {
                        existing.name == name
                            && (existing.declaring == *class
                                || existing.visibility != Visibility::Private)
                    }) {
                        if existing.declaring == *class {
                            let origin = property_origins.get(name).copied().unwrap_or(class);
                            return Err(incompatible(
                                "property",
                                &format!("${name}"),
                                class,
                                origin,
                                &provider.name,
                            ));
                        }
                        let (previous, next) = if property.static_storage.is_some() {
                            ("non static", "static")
                        } else {
                            ("static", "non static")
                        };
                        return Err(NativeError::error(format!(
                            "Cannot redeclare {previous} {}::${name} as {next} {class}::${name}",
                            existing.declaring,
                        )));
                    }
                }
            }
            for (properties, target) in [
                (&provider.defaults, &mut *defaults),
                (&provider.static_properties, &mut *statics),
            ] {
                for property in properties {
                    let name = property.name.as_str();
                    let key = property_key(class, property);
                    let same_name = |candidate: &PropertyDefault| {
                        candidate.name == name && candidate.declaring == *class
                    };
                    if let Some(existing) = target.iter().find(|candidate| same_name(candidate)) {
                        let agree = self.with_trait_constants(class, constants, |vm| {
                            vm.trait_properties_agree(existing, property, class, unit)
                        })?;
                        if !agree {
                            let origin = property_origins.get(name).copied().unwrap_or(class);
                            return Err(incompatible(
                                "property",
                                &format!("${name}"),
                                class,
                                origin,
                                &provider.name,
                            ));
                        }
                        continue;
                    }
                    if declaration.is_readonly && !property.is_readonly {
                        let class = display(class);
                        return Err(NativeError::error(format!(
                            "Readonly class {class} cannot use trait with a non-readonly property {}::${}",
                            provider.name, property.name,
                        )));
                    }
                    super::declaration_errors::check_property_visibility(
                        display(class),
                        property.visibility,
                        name,
                        target,
                    )?;
                    if let Some(inherited) = target.iter().find(|inherited| {
                        inherited.name == name && inherited.visibility != Visibility::Private
                    }) && !self.trait_types_match(
                        property.ty.as_ref(),
                        class,
                        inherited.ty.as_ref(),
                        &inherited.declaring,
                        unit,
                    ) {
                        let expected = inherited
                            .ty
                            .as_ref()
                            .map_or("omitted", |ty| ty.display.as_str());
                        return Err(NativeError::error(format!(
                            "Type of {class}::${name} must be {expected} (as in class {})",
                            inherited.declaring,
                        )));
                    }
                    let mut linked = property.clone();
                    linked.key = key.clone();
                    linked.declaring = class.clone();
                    linked.static_storage = property.static_storage.as_ref().map(|_| {
                        crate::reference_holders::StaticProperty::new(property.value.clone())
                    });
                    linked.static_initialized = Rc::new(Cell::new(false));
                    linked.value_initialized = Rc::new(Cell::new(property.initialized));
                    if let Some(inherited) =
                        target.iter_mut().find(|candidate| candidate.key == key)
                    {
                        *inherited = linked;
                    } else {
                        target.push(linked);
                    }
                    property_origins.insert(name, &provider.name);
                }
            }
        }

        Ok(())
    }
}

fn property_key(class: &str, property: &PropertyDefault) -> ArrayKey {
    if property.visibility == Visibility::Private {
        Object::private_key(class, &property.name)
    } else {
        property.key.clone()
    }
}

fn incompatible(kind: &str, name: &str, class: &str, origin: &str, provider: &str) -> NativeError {
    let class = display(class);
    let origin = display(origin);
    NativeError::error(format!(
        "{origin} and {provider} define the same {kind} ({name}) in the composition of {class}. However, the definition differs and is considered incompatible. Class was composed",
    ))
}
