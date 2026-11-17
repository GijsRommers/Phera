//! A reference remains constrained while a live typed property still holds its cell.

use crate::{Vm, classes::class::PropertyDefault};
use phera_extension_api::NativeResult;
use phera_value::Value;
use std::{cell::RefCell, rc::Rc};

impl Vm {
    /// Only reference writes pay for discovering live property constraints.
    #[inline(never)]
    pub(crate) fn check_reference_assignment(
        &mut self,
        reference: &Rc<RefCell<Value>>,
        mut value: Value,
    ) -> NativeResult {
        let mut constraints = Vec::new();
        for object in self
            .classes
            .objects
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            let Some(class) = self.classes.get(object.class.name()) else {
                continue;
            };
            for property in &class.defaults {
                if property.ty.is_some()
                    && let Some(Value::Reference(cell)) = object.get(&property.key)
                    && Rc::ptr_eq(reference, &cell)
                {
                    constraints.push(property.clone());
                }
            }
        }
        for class in self.classes.entries.iter() {
            for property in &class.static_properties {
                if property.ty.is_some()
                    && let Value::Reference(cell) = &property.value
                    && Rc::ptr_eq(reference, cell)
                {
                    constraints.push(property.clone());
                }
            }
        }
        for property in constraints {
            value = self.coerce_reference_property(value, &property)?;
        }
        Ok(value)
    }

    /// The alias diagnostic names the property holding the type constraint, not the variable.
    fn coerce_reference_property(
        &mut self,
        value: Value,
        property: &PropertyDefault,
    ) -> NativeResult {
        let Some(declaration) = &property.ty else {
            return Ok(value);
        };
        let actual = match &value {
            Value::Object(object) => object.class.name().to_owned(),
            _ => value.type_name(),
        };
        self.coerce_property(value, property, declaration)
            .map_err(|_| {
                phera_extension_api::NativeError::type_error(format!(
                    "Cannot assign {actual} to reference held by property {}::${} of type {}",
                    property.declaring, property.name, declaration.display
                ))
            })
    }
}

impl Vm {
    /// A cell with no external aliases can collapse back into its owning property.
    pub(crate) fn release_property_reference(&self, reference: Rc<RefCell<Value>>) {
        if Rc::strong_count(&reference) != 2 {
            return;
        }
        for object in self
            .classes
            .objects
            .iter()
            .filter_map(std::rc::Weak::upgrade)
        {
            let key = object
                .to_array()
                .iter()
                .find_map(|(key, value)| match value {
                    Value::Reference(cell) if Rc::ptr_eq(cell, &reference) => Some(key.clone()),
                    _ => None,
                });
            if let Some(key) = key {
                if let Ok(value) = reference.try_borrow() {
                    object.set(key, value.clone());
                }
                return;
            }
        }
    }
}
