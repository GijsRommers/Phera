//! `$object->items['key'] = 1` writes through a getter's reference or object.
//! A getter returning an array by value gives a detached temporary.

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;

use crate::access_mode::{Binding, Creation, Missing, Mutation};
use crate::inline_stack::InlineStack;
use crate::magic_properties::{PropertyHook, PropertyRead};
use crate::path_update::PathKey;
use crate::{AccessPath, Vm};

pub(crate) enum PropertyMutation {
    Write(Value, Binding),
    Unset,
}

enum OverloadedMutation {
    Ordinary,
    Handled,
    Detached(Value, InlineStack<PathKey>),
}

impl Vm {
    pub(crate) async fn mutate_magic_place(
        &mut self,
        place: &AccessPath,
        operation: PropertyMutation,
    ) -> Result<(), NativeError> {
        match self.plan_magic_mutation(place, &operation).await? {
            OverloadedMutation::Ordinary => match operation {
                PropertyMutation::Write(value, binding) => self.write_place(place, value, binding),
                PropertyMutation::Unset => self.unset_place(place),
            },
            OverloadedMutation::Handled => Ok(()),
            OverloadedMutation::Detached(mut root, keys) => {
                let (mut value, mutation) = match operation {
                    PropertyMutation::Write(value, binding) => (value, binding.into()),
                    PropertyMutation::Unset => (Value::Null, Mutation::Unset),
                };
                self.validate_property_mutation(root.deref().clone(), &keys, &mut value, mutation)?;
                if mutation == Mutation::Assign {
                    value = self.check_path_reference_assignment(&root, &keys, value)?;
                }
                if mutation == Mutation::Unset
                    && matches!(keys.iter().next(), Some(PathKey::Index(_)))
                    && !root.is_array()
                {
                    if root.is_null() {
                        return Ok(());
                    }
                    return Err(NativeError::error(
                        "Cannot unset offset in a non-array variable",
                    ));
                }
                let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
                crate::path_update::update(
                    &mut root,
                    &keys,
                    value,
                    mutation,
                    &mut diagnostics.buffer,
                    &diagnostics.vm.property_references,
                )
            }
        }
    }

    async fn plan_magic_mutation(
        &mut self,
        place: &AccessPath,
        operation: &PropertyMutation,
    ) -> Result<OverloadedMutation, NativeError> {
        let Some(register) = self.base_register(place, Creation::Existing, Missing::Silent)? else {
            return Ok(OverloadedMutation::Ordinary);
        };
        let mut cursor = self.read_root(register, Missing::Silent);
        let mut detached = None;
        let mut resolved = InlineStack::new();
        let keys = self.keys(place)?;
        let mut steps = keys.iter().peekable();
        while let Some(key) = steps.next() {
            let current = cursor.deref().clone();
            if let (PathKey::Property(name), Value::Object(object)) = (key, &current) {
                let class = self
                    .classes
                    .for_object(&object)
                    .ok_or_else(|| internal_error!("property class"))?;
                let display = crate::path_update::property_name(name);
                let (stored, slot) = self.property_slot(object, &display);
                let last = steps.peek().is_none();
                if stored.is_none()
                    && class
                        .magic
                        .as_ref()
                        .is_some_and(|magic| magic.has_properties())
                    && !slot
                        .as_ref()
                        .is_some_and(|key| self.typed_property_uninitialized(&class, &object, key))
                {
                    let (hook, argument) = match (last, operation) {
                        (true, PropertyMutation::Write(value, Binding::ByValue)) => {
                            (PropertyHook::Set, Some(value.clone()))
                        }
                        (true, PropertyMutation::Unset) => (PropertyHook::Unset, None),
                        _ => (PropertyHook::Get, None),
                    };
                    if let Some(method) = self.property_hook(&class, &object, name, hook) {
                        let value = self
                            .invoke_property_hook(
                                class,
                                object.clone(),
                                hook,
                                name.clone(),
                                method,
                                argument,
                            )
                            .await?;
                        if hook != PropertyHook::Get {
                            return Ok(OverloadedMutation::Handled);
                        }
                        self.warn_indirect_magic_property(&object, &display, &value);
                        if last {
                            return Err(NativeError::error(
                                "Cannot assign by reference to overloaded object",
                            ));
                        }
                        cursor = value.clone();
                        detached = Some(value);
                        resolved = InlineStack::new();
                        continue;
                    }
                }
                let key = match slot {
                    Some(key) => key,
                    None => self.property_key(&object, &display)?,
                };
                cursor = stored.unwrap_or_default();
                resolved.push(PathKey::Property(key));
            } else {
                cursor = match (key, &current) {
                    (PathKey::Index(Some(key)), Value::Array(array)) => {
                        array.get(key).cloned().unwrap_or_default()
                    }
                    _ => Value::Null,
                };
                resolved.push(key.clone());
            }
        }
        Ok(match detached {
            Some(value) => OverloadedMutation::Detached(value, resolved),
            None => OverloadedMutation::Ordinary,
        })
    }

    /// The alias returned by &__get points to the getter's own storage.
    pub(crate) async fn magic_reference(&mut self, place: &AccessPath) -> NativeResult {
        let Some(register) = self.base_register(place, Creation::Existing, Missing::Silent)? else {
            return self.reference(place);
        };
        let keys = self.keys(place)?;
        let mut cursor = self.read_root(register, Missing::Silent);
        let mut steps = keys.iter().peekable();
        while let Some(key) = steps.next() {
            cursor = match key {
                PathKey::Property(key) => {
                    let Value::Object(object) = cursor.deref().clone() else {
                        break;
                    };
                    let display = crate::path_update::property_name(key);
                    let (stored, slot) = self.property_slot(&object, &display);
                    if let Some(value) = stored {
                        value.deref().clone()
                    } else if let Some(class) = self.classes.for_object(&object)
                        && !slot.as_ref().is_some_and(|key| {
                            self.typed_property_uninitialized(&class, &object, key)
                        })
                        && self
                            .property_hook(&class, &object, key, PropertyHook::Get)
                            .is_some()
                    {
                        let value = self
                            .read_overloaded_property(
                                Value::Object(object.clone()),
                                key.clone(),
                                PropertyRead::Read,
                            )
                            .await?;
                        self.warn_indirect_magic_property(&object, &display, &value);
                        if steps.peek().is_none() {
                            return Ok(match value {
                                Value::Reference(_) => value,
                                _ => Value::Reference(Rc::new(RefCell::new(value))),
                            });
                        }
                        return self.reference_from_magic(value, steps.cloned());
                    } else {
                        Value::Null
                    }
                }
                PathKey::Index(Some(key)) => self.read_path_index(cursor, key, Missing::Silent)?,
                PathKey::Index(None) => Value::Null,
            };
        }
        self.reference(place)
    }

    fn warn_indirect_magic_property(
        &mut self,
        object: &phera_value::Object,
        name: &str,
        value: &Value,
    ) {
        if !value.is_reference() && !value.is_object() {
            self.diagnostic(
                "Notice",
                format!(
                    "Indirect modification of overloaded property {}::${name} has no effect",
                    object.class.name()
                ),
            );
        }
    }

    fn reference_from_magic(
        &mut self,
        mut root: Value,
        tail: impl Iterator<Item = PathKey>,
    ) -> NativeResult {
        let mut keys = InlineStack::new();
        let mut value = root.deref().clone();
        for key in tail {
            value = match key {
                PathKey::Property(key) => {
                    let (resolved, target) = match &value {
                        Value::Object(object) => {
                            let resolved = self
                                .property_key(object, &crate::path_update::property_name(&key))?;
                            let target = crate::property_storage::read(object, &resolved)
                                .unwrap_or_default();
                            (resolved, target)
                        }
                        _ => (key, Value::Null),
                    };
                    keys.push(PathKey::Property(resolved));
                    target
                }
                PathKey::Index(key) => {
                    let value = match (&*value.deref(), &key) {
                        (Value::Array(array), Some(key)) => {
                            array.get(key).cloned().unwrap_or_default()
                        }
                        _ => Value::Null,
                    };
                    keys.push(PathKey::Index(key));
                    value
                }
            };
        }
        if value.is_reference() {
            return Ok(value);
        }
        let mut reference = Value::Reference(Rc::new(RefCell::new(value)));
        self.validate_property_mutation(
            root.deref().clone(),
            &keys,
            &mut reference,
            Mutation::Bind,
        )?;
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        crate::path_update::update(
            &mut root,
            &keys,
            reference.clone(),
            Mutation::Bind,
            &mut diagnostics.buffer,
            &diagnostics.vm.property_references,
        )?;
        Ok(reference)
    }
}
