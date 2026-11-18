//! Matching trait members compare equivalent types and evaluated defaults.

use std::{collections::HashMap, rc::Rc};

use phera_bytecode::CompiledUnit;
use phera_extension_api::NativeError;
use phera_value::Value;

use super::{
    LinkedConstant,
    class::{ConstantInitializer, PropertyDefault, literal_initializer},
};
use crate::Vm;

impl Vm {
    pub(super) fn with_trait_constants<T>(
        &mut self,
        name: &str,
        constants: &HashMap<String, Rc<LinkedConstant>>,
        compare: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = self
            .initializer_class
            .replace(crate::initializers::InitializerClass {
                name: name.to_owned(),
                constants: constants.clone(),
            });
        let result = compare(self);
        self.initializer_class = previous;
        result
    }

    pub(super) fn trait_types_match(
        &self,
        existing: Option<&phera_bytecode::Type>,
        existing_scope: &str,
        incoming: Option<&phera_bytecode::Type>,
        incoming_scope: &str,
        unit: &CompiledUnit,
    ) -> bool {
        match (existing, incoming) {
            (None, None) => true,
            (Some(left), Some(right)) => {
                self.type_subset(
                    Some(&left.display),
                    existing_scope,
                    Some(&right.display),
                    incoming_scope,
                    unit,
                ) && self.type_subset(
                    Some(&right.display),
                    incoming_scope,
                    Some(&left.display),
                    existing_scope,
                    unit,
                )
            }
            _ => false,
        }
    }

    pub(super) fn trait_properties_agree(
        &mut self,
        existing: &PropertyDefault,
        incoming: &PropertyDefault,
        class: &str,
        unit: &CompiledUnit,
    ) -> Result<bool, NativeError> {
        if existing.visibility != incoming.visibility
            || existing.is_readonly != incoming.is_readonly
            || existing.initialized != incoming.initialized
            || !self.trait_types_match(
                existing.ty.as_ref(),
                class,
                incoming.ty.as_ref(),
                class,
                unit,
            )
        {
            return Ok(false);
        }
        let left = self.trait_property_default(existing, class)?;
        let right = self.trait_property_default(incoming, class)?;
        Ok(identical(left, right))
    }

    pub(super) fn trait_property_default(
        &mut self,
        property: &PropertyDefault,
        class: &str,
    ) -> Result<Option<Value>, NativeError> {
        let value = match property.initializer {
            Some(initializer) => self.evaluate_initializer(&initializer, class),
            None => Ok(property.value.clone()),
        };
        comparison_default(
            value.map(|value| super::class::widen_property_value(value, property.ty.as_ref())),
        )
    }

    pub(super) fn trait_constants_agree(
        &mut self,
        existing: &LinkedConstant,
        incoming: &LinkedConstant,
        class: &str,
        unit: &CompiledUnit,
    ) -> Result<bool, NativeError> {
        if existing.visibility != incoming.visibility
            || existing.is_final != incoming.is_final
            || !self.trait_types_match(
                existing.ty.as_ref(),
                class,
                incoming.ty.as_ref(),
                class,
                unit,
            )
        {
            return Ok(false);
        }
        let left = self.trait_constant_default(existing, class)?;
        let right = self.trait_constant_default(incoming, class)?;
        Ok(identical(left, right))
    }

    pub(super) fn trait_constant_default(
        &mut self,
        constant: &LinkedConstant,
        class: &str,
    ) -> Result<Option<Value>, NativeError> {
        match &constant.initializer {
            ConstantInitializer::Compiled { function, .. } => {
                if let Some(value) = literal_initializer(&self.code, function) {
                    Ok(Some(value))
                } else {
                    comparison_default(self.evaluate_initializer(function, class))
                }
            }
            ConstantInitializer::Value(value) => Ok(Some(value.clone())),
            ConstantInitializer::EnumCase { .. } => Ok(None),
        }
    }
}

fn comparison_default(result: Result<Value, NativeError>) -> Result<Option<Value>, NativeError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if crate::autoload::ClassMiss::is(&error) => Err(error),
        Err(_) => Ok(None),
    }
}

fn identical(left: Option<Value>, right: Option<Value>) -> bool {
    let (Some(left), Some(right)) = (left, right) else {
        return false;
    };
    matches!(
        phera_value::strict_equals(&left, &right, &mut phera_value::NoDiagnostics),
        Ok(Value::Bool(true))
    )
}
