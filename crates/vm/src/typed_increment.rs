//! `++$c->n` at PHP_INT_MAX can produce a float only if its typed slot allows one.

use crate::{
    AccessPath, Vm, classes::class::PropertyDefault, inline_stack::InlineStack,
    path_update::PathKey, typed_references::reference_property_name,
};
use phera_bytecode::{IncDec, PathBase, Reg, TypeMask};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Object, Value};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy)]
pub(crate) enum IntegerLimit {
    Maximum,
    Minimum,
}

impl IntegerLimit {
    /// Ordinary arithmetic and string-to-number conversions use their normal assignment rules.
    pub(crate) fn reached(old: &Value, new: &Value, mode: IncDec) -> Option<Self> {
        if !matches!(new, Value::Float(_)) {
            return None;
        }
        match (mode, old) {
            (IncDec::PreIncrement | IncDec::PostIncrement, Value::Int(i64::MAX)) => {
                Some(Self::Maximum)
            }
            (IncDec::PreDecrement | IncDec::PostDecrement, Value::Int(i64::MIN)) => {
                Some(Self::Minimum)
            }
            _ => None,
        }
    }

    fn error(self, property: &PropertyDefault, reference: bool) -> NativeError {
        let (operation, bound) = match self {
            Self::Maximum => ("increment", "maximal"),
            Self::Minimum => ("decrement", "minimal"),
        };
        let subject = if reference {
            "a reference held by property"
        } else {
            "property"
        };
        NativeError::type_error(format!(
            "Cannot {operation} {subject} {} past its {bound} value",
            reference_property_name(property)
        ))
    }
}

fn rejects_float(property: &PropertyDefault) -> bool {
    property
        .ty
        .as_ref()
        .is_some_and(|ty| !ty.mask.intersects(TypeMask::FLOAT))
}

impl Vm {
    #[cold]
    pub(crate) fn check_increment_reference(
        &self,
        cell: &Rc<RefCell<Value>>,
        limit: IntegerLimit,
    ) -> Result<(), NativeError> {
        let references = self.property_references.borrow();
        if let Some(properties) = references.constraints.get(&(Rc::as_ptr(cell) as usize))
            && let Some(property) = properties.values().find(|property| rejects_float(property))
        {
            return Err(limit.error(property, true));
        }
        Ok(())
    }

    #[cold]
    pub(crate) fn check_increment_target(
        &self,
        root: &Value,
        keys: &InlineStack<PathKey>,
        limit: IntegerLimit,
    ) -> Result<(), NativeError> {
        if let Some(cell) = crate::typed_references::path_reference(root, keys) {
            self.check_increment_reference(&cell, limit)?;
        }
        Ok(())
    }

    #[cold]
    pub(crate) fn check_increment_place(
        &mut self,
        place: &AccessPath,
        register: Reg,
        keys: &InlineStack<PathKey>,
        limit: IntegerLimit,
    ) -> Result<(), NativeError> {
        self.check_increment_target(&self.raw_local(register), keys, limit)?;
        if keys.iter().next().is_none()
            && let PathBase::StaticProperty { class, name, .. } = place.base()?
        {
            let (class, name) = self.static_property_names(class, name)?;
            if let Some(class) = self.classes.get(&class)
                && let Some(property) = class
                    .static_properties
                    .iter()
                    .rev()
                    .find(|p| p.name == name)
                && rejects_float(property)
            {
                return Err(limit.error(property, false));
            }
        }
        Ok(())
    }

    #[cold]
    pub(crate) fn check_increment_property(
        &mut self,
        object: &Object,
        name: &ArrayKey,
        limit: IntegerLimit,
    ) -> Result<(), NativeError> {
        let display = crate::path_update::property_name(name);
        let Ok(key) = self.property_key(object, &display) else {
            return Ok(());
        };
        if let Some(Value::Reference(cell)) = crate::property_storage::read(object, &key) {
            self.check_increment_reference(&cell, limit)?;
        }
        if let Some(class) = self.classes.for_object(object)
            && let Some(property) = class.defaults.iter().find(|p| p.key == key)
            && rejects_float(property)
        {
            return Err(limit.error(property, false));
        }
        Ok(())
    }
}
