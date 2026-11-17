//! Property and array mutations share iterative path reconstruction and PHP reference binding.

use crate::access_mode::{Binding, Creation, Missing, Mutation};
use crate::{
    AccessPath, Vm,
    path_update::{PathKey, update},
};
use phera_bytecode::{PathBase, Step};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;
use std::{cell::RefCell, rc::Rc};

impl Vm {
    /// Separate shared arrays on writes while preserving reference-cell identity.
    pub(crate) fn write_place(
        &mut self,
        place: &AccessPath,
        mut value: Value,
        binding: Binding,
    ) -> Result<(), NativeError> {
        let register = self
            .base_register(place, Creation::Create, Missing::Silent)?
            .ok_or_else(|| internal_error!("missing place"))?;
        // Appending to an existing array needs neither key conversion nor a
        // parent reconstruction stack. References retain the general handler.
        let mut steps = place.steps()?;
        if matches!(steps.next(), Some(Step::Append))
            && steps.next().is_none()
            && let Some(Value::Array(array)) = self
                .call_stack
                .current_mut()
                .and_then(|frame| {
                    frame
                        .slots_mut(&mut self.registers)
                        .get_mut(register.index())
                })
                .and_then(Option::as_mut)
        {
            Rc::make_mut(array).push(value)?;
            return Ok(());
        }
        if let PathBase::StaticProperty { class, name, .. } = place.base()?
            && place.steps()?.next().is_none()
        {
            self.validate_static_mutation(class, name, &mut value)?;
        }
        // Mirrors property_keys and validation: index-only paths cannot cross an
        // object property boundary, so retain the original array-only key path.
        let keys = if place
            .steps()?
            .all(|step| matches!(step, Step::Index(_) | Step::Append))
        {
            self.keys(place)?
        } else {
            let root_value = self.read_root(register, Missing::Silent);
            let keys = self.property_keys(place, root_value.clone())?;
            self.validate_property_mutation(root_value, &keys, &mut value, binding.into())?;
            keys
        };
        let root = self
            .call_stack
            .current_mut()
            .and_then(|frame| {
                frame
                    .slots_mut(&mut self.registers)
                    .get_mut(register.0 as usize)
            })
            .and_then(Option::take)
            .unwrap_or_default();
        let previous_reference = if keys.iter().next().is_none() && binding == Binding::ByReference
        {
            match &root {
                Value::Reference(reference) => Some(reference.clone()),
                _ => None,
            }
        } else {
            None
        };
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let updated = update(root, &keys, value, binding.into(), &mut diagnostics.buffer);
        drop(diagnostics);
        let updated = match updated {
            Ok(updated) => updated,
            Err(failure) => {
                self.set(register, failure.root);
                return Err(failure.error);
            }
        };
        if let PathBase::GlobalNamed { name, .. } = place.base()? {
            let name = self.get(name);
            if let Value::String(name) = name {
                self.publish_global(name.to_string_lossy(), updated.clone());
            }
        }
        self.set(register, updated);
        if let Some(reference) = previous_reference {
            self.release_property_reference(reference);
        }
        Ok(())
    }

    /// Promote a slot to a shared reference cell only when an alias is requested.
    pub(crate) fn reference(&mut self, place: &AccessPath) -> NativeResult {
        let value = self.read_place(place, crate::access_mode::Missing::Silent)?;
        let register = self
            .base_register(place, Creation::Create, Missing::Silent)?
            .ok_or_else(|| internal_error!("reference missing"))?;
        let mut raw = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.0 as usize))
            .cloned()
            .flatten()
            .unwrap_or_default();
        let keys = self.property_keys(place, raw.deref().clone())?;
        for key in keys.iter().cloned() {
            let plain = raw.deref().clone();
            raw = match (plain, key) {
                (Value::Array(array), PathKey::Index(Some(key))) => {
                    array.get(&key).cloned().unwrap_or_default()
                }
                (Value::Object(object), PathKey::Property(key)) => {
                    crate::property_storage::read(&object, &key).unwrap_or_default()
                }
                _ => Value::Null,
            };
        }
        if matches!(raw, Value::Reference(_)) {
            return Ok(raw);
        }
        let reference = Value::Reference(Rc::new(RefCell::new(value)));
        self.write_place(
            place,
            reference.clone(),
            crate::access_mode::Binding::ByReference,
        )?;
        Ok(reference)
    }

    /// Remove the slot itself without assigning null through an existing alias.
    pub(crate) fn unset_place(&mut self, place: &AccessPath) -> Result<(), NativeError> {
        if let PathBase::StaticProperty { class, name, .. } = place.base()?
            && place.steps()?.next().is_none()
        {
            let (class, name) = self.static_property_names(class, name)?;
            return Err(NativeError::error(format!(
                "Attempt to unset static property {class}::${name}"
            )));
        }
        let Some(register) = self.base_register(place, Creation::Existing, Missing::Silent)? else {
            return Ok(());
        };
        if place.steps()?.next().is_none() {
            if let PathBase::GlobalNamed { name, .. } = place.base()? {
                let name = self.get(name).deref().clone();
                if let Value::String(name) = name {
                    self.remove_global(&name.to_string_lossy());
                }
            }
            self.clear_local(register)?;
            return Ok(());
        }
        let root_value = self.read_root(register, Missing::Silent);
        let keys = self.property_keys(place, root_value.clone())?;
        self.validate_property_mutation(root_value, &keys, &mut Value::Null, Mutation::Unset)?;
        let root = self
            .call_stack
            .current_mut()
            .and_then(|frame| {
                frame
                    .slots_mut(&mut self.registers)
                    .get_mut(register.0 as usize)
            })
            .and_then(Option::take)
            .unwrap_or_default();
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let updated = update(
            root,
            &keys,
            Value::Null,
            Mutation::Unset,
            &mut diagnostics.buffer,
        );
        drop(diagnostics);
        let updated = match updated {
            Ok(updated) => updated,
            Err(failure) => {
                self.set(register, failure.root);
                return Err(failure.error);
            }
        };
        self.set(register, updated);
        Ok(())
    }
}
