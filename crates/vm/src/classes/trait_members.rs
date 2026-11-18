//! A trait's declared properties and constants belong to each consuming class.

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

use phera_bytecode::{ClassDeclaration, Visibility};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object, Value};

use super::{
    Class, ConstantState, LinkedConstant,
    class::{ConstantInitializer, PropertyDefault, literal_initializer},
    trait_methods::display,
};
use crate::Vm;

impl Vm {
    pub(super) fn compose_trait_members(
        &mut self,
        declaration: &ClassDeclaration,
        traits: &[Rc<Class>],
        defaults: &mut Vec<PropertyDefault>,
        statics: &mut Vec<PropertyDefault>,
        constants: &mut HashMap<String, Rc<LinkedConstant>>,
    ) -> Result<(), NativeError> {
        let class = &declaration.name;
        let mut property_origins = HashMap::new();
        let mut constant_origins = HashMap::new();
        for provider in traits {
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
                        if !self.trait_properties_agree(existing, property) {
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
                            provider.name, &property.name,
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
            let mut trait_constants: Vec<_> = provider.constants.iter().collect();
            trait_constants.sort_by_key(|(name, _)| *name);
            for (name, constant) in trait_constants {
                if let Some(existing) = constants.get(name)
                    && existing.declaring == *class
                {
                    if !self.trait_constants_agree(existing, constant) {
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
        Ok(())
    }

    fn trait_properties_agree(
        &mut self,
        existing: &PropertyDefault,
        incoming: &PropertyDefault,
    ) -> bool {
        if existing.visibility != incoming.visibility
            || existing.ty != incoming.ty
            || existing.is_readonly != incoming.is_readonly
            || existing.initialized != incoming.initialized
        {
            return false;
        }
        let left = self.trait_property_default(existing);
        let right = self.trait_property_default(incoming);
        identical(left, right)
    }

    fn trait_property_default(&mut self, property: &PropertyDefault) -> Option<Value> {
        match property.initializer {
            Some(initializer) => self
                .evaluate_initializer(&initializer, &property.declaring)
                .ok(),
            None => Some(property.value.clone()),
        }
    }

    fn trait_constants_agree(
        &mut self,
        existing: &LinkedConstant,
        incoming: &LinkedConstant,
    ) -> bool {
        if existing.visibility != incoming.visibility
            || existing.ty != incoming.ty
            || existing.is_final != incoming.is_final
        {
            return false;
        }
        let left = self.trait_constant_default(existing);
        let right = self.trait_constant_default(incoming);
        identical(left, right)
    }

    fn trait_constant_default(&mut self, constant: &LinkedConstant) -> Option<Value> {
        match &constant.initializer {
            ConstantInitializer::Compiled { function, .. } => {
                literal_initializer(&self.code, function).or_else(|| {
                    self.evaluate_initializer(function, &constant.declaring)
                        .ok()
                })
            }
            ConstantInitializer::Value(value) => Some(value.clone()),
            ConstantInitializer::EnumCase { .. } => None,
        }
    }
}

fn property_key(class: &str, property: &PropertyDefault) -> ArrayKey {
    if property.visibility == Visibility::Private {
        Object::private_key(class, &property.name)
    } else {
        property.key.clone()
    }
}

fn identical(left: Option<Value>, right: Option<Value>) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    matches!(
        phera_value::strict_equals(
            &left,
            &right,
            &mut phera_value::CollectDiagnostics::default()
        ),
        Ok(Value::Bool(true))
    )
}

fn incompatible(kind: &str, name: &str, class: &str, origin: &str, provider: &str) -> NativeError {
    let class = display(class);
    let origin = display(origin);
    NativeError::error(format!(
        "{origin} and {provider} define the same {kind} ({name}) in the composition of {class}. However, the definition differs and is considered incompatible. Class was composed",
    ))
}
