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
            && let Some(Value::Array(array)) =
                self.current_slot_mut(register).and_then(Option::as_mut)
        {
            Rc::make_mut(array).push(value)?;
            return Ok(());
        }
        if let PathBase::StaticProperty { class, name, .. } = place.base()?
            && place.steps()?.next().is_none()
        {
            self.validate_static_mutation(class, name, &mut value)?;
            if binding == Binding::ByReference {
                self.bind_static_property_reference(class, name, value)?;
                return Ok(());
            }
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
            if self.validate_property_mutation(root_value, &keys, &mut value, binding.into())? {
                return Ok(());
            }
            keys
        };
        self.write_resolved_place(place, register, keys, value, binding)
    }

    pub(crate) fn write_resolved_place(
        &mut self,
        place: &AccessPath,
        register: phera_bytecode::Reg,
        keys: crate::inline_stack::InlineStack<PathKey>,
        mut value: Value,
        binding: Binding,
    ) -> Result<(), NativeError> {
        let mut root = self
            .current_slot_mut(register)
            .and_then(Option::take)
            .unwrap_or_default();
        if binding == Binding::ByValue {
            match self.check_path_reference_assignment(&root, &keys, value) {
                Ok(converted) => value = converted,
                Err(error) => {
                    self.set(register, root);
                    return Err(error);
                }
            }
        }
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
        let result = update(
            &mut root,
            &keys,
            value,
            binding.into(),
            &mut diagnostics.buffer,
            &diagnostics.vm.property_references,
        );
        drop(diagnostics);
        if let Err(error) = result {
            self.set(register, root);
            return Err(error);
        }
        let updated = root;
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
        let register = self
            .base_register(place, Creation::Create, Missing::Silent)?
            .ok_or_else(|| internal_error!("reference missing"))?;
        let keys = self.property_keys(place, self.raw_local(register).deref().clone())?;
        self.reference_resolved(place, register, keys)
    }

    pub(crate) fn reference_resolved(
        &mut self,
        place: &AccessPath,
        register: phera_bytecode::Reg,
        keys: crate::inline_stack::InlineStack<PathKey>,
    ) -> NativeResult {
        let mut raw = self.raw_local(register);
        let mut steps = keys.iter().cloned().peekable();
        while let Some(key) = steps.next() {
            let plain = raw.deref().clone();
            raw = match (plain, key) {
                (Value::Array(array), PathKey::Index(Some(key))) => {
                    array.get(&key).cloned().unwrap_or_default()
                }
                (Value::Object(object), PathKey::Property(key)) => {
                    if steps.peek().is_none()
                        && let Some(access) = crate::properties::native_property_access(&object)
                        && (access.mutate)(
                            self,
                            &object,
                            &crate::path_update::property_name(&key),
                            phera_extension_api::NativePropertyMutation::Reference,
                        )?
                    {
                        let value = self
                            .native_property_read(
                                &object,
                                &crate::path_update::property_name(&key),
                                Missing::Silent,
                            )?
                            .unwrap_or_default();
                        return Ok(Value::Reference(Rc::new(RefCell::new(value))));
                    }
                    crate::property_storage::read(&object, &key).unwrap_or_default()
                }
                _ => Value::Null,
            };
        }
        drop(steps);
        if matches!(raw, Value::Reference(_)) {
            if let PathBase::StaticProperty { class, name, .. } = place.base()?
                && place.steps()?.next().is_none()
            {
                self.track_static_property_reference(class, name)?;
            }
            return Ok(raw);
        }
        let reference = Value::Reference(Rc::new(RefCell::new(raw)));
        if keys.iter().next().is_none() {
            self.write_place(place, reference.clone(), Binding::ByReference)?;
        } else {
            let current = self.read_root(register, Missing::Silent);
            let mut pending = reference.clone();
            if !self.validate_property_mutation(current, &keys, &mut pending, Mutation::Bind)? {
                self.write_resolved_place(place, register, keys, pending, Binding::ByReference)?;
            }
        }
        Ok(reference)
    }

    /// Remove the slot itself without assigning null through an existing alias.
    pub(crate) fn unset_place(&mut self, place: &AccessPath) -> Result<(), NativeError> {
        if let PathBase::StaticProperty { class, name, .. } = place.base()?
            && place.steps()?.next().is_none()
        {
            let (class, name) = self.static_property_names(class, name)?;
            let class = self
                .classes
                .get(&class)
                .ok_or_else(|| Self::class_miss(&class, "Class"))?;
            return Err(NativeError::error(format!(
                "Attempt to unset static property {}::${name}",
                class.name
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
        let mut current = self.read_root(register, Missing::Silent);
        let mut keys = crate::inline_stack::InlineStack::new();
        for step in place.steps()? {
            let Step::Index(index) = step else {
                return Err(internal_error!("unset dimension path"));
            };
            let Some(key) = self.unset_index_key(&current, index)? else {
                return Ok(());
            };
            current = self.read_path_index(current, &key, Missing::Silent)?;
            keys.push(PathKey::Index(Some(key)));
        }
        self.unset_resolved_place(register, &keys)
    }

    pub(crate) fn unset_resolved_place(
        &mut self,
        register: phera_bytecode::Reg,
        keys: &crate::inline_stack::InlineStack<PathKey>,
    ) -> Result<(), NativeError> {
        let current = self.read_root(register, Missing::Silent);
        if self.validate_property_mutation(current, keys, &mut Value::Null, Mutation::Unset)? {
            return Ok(());
        }
        let mut root = self
            .current_slot_mut(register)
            .and_then(Option::take)
            .unwrap_or_default();
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let result = update(
            &mut root,
            keys,
            Value::Null,
            Mutation::Unset,
            &mut diagnostics.buffer,
            &diagnostics.vm.property_references,
        );
        drop(diagnostics);
        if let Err(error) = result {
            self.set(register, root);
            return Err(error);
        }
        let updated = root;
        self.set(register, updated);
        Ok(())
    }
}
