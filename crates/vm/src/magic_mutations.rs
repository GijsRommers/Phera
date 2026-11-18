//! `$object->items['key'] = 1` writes through a getter's reference or object.
//! A getter returning an array by value gives a detached temporary.

use phera_bytecode::{Reg, Step};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::access_mode::{Binding, Creation, Missing, Mutation};
use crate::inline_stack::InlineStack;
use crate::magic_properties::PropertyHook;
use crate::path_update::PathKey;
use crate::{AccessPath, Vm};

mod write_value;
pub(crate) use write_value::WriteValue;

pub(crate) enum PropertyMutation {
    Write(WriteValue, Binding),
    Unset,
}

enum OverloadedMutation {
    Ordinary(Reg, Box<InlineStack<PathKey>>),
    Unresolved,
    Handled,
    Converted(Value),
    Detached(Value, Box<InlineStack<PathKey>>),
}

impl Vm {
    pub(crate) async fn mutate_magic_place(
        &mut self,
        place: &AccessPath,
        mut operation: PropertyMutation,
    ) -> Result<Option<Value>, NativeError> {
        let plan = self.plan_magic_mutation(place, &mut operation).await?;
        match plan {
            OverloadedMutation::Ordinary(register, keys) => match operation {
                PropertyMutation::Write(value, binding) => {
                    let mut value = value.into_value(self);
                    let current = self.read_root(register, Missing::Silent);
                    if !self.validate_property_mutation(
                        current,
                        &keys,
                        &mut value,
                        binding.into(),
                    )? {
                        self.write_resolved_place(place, register, *keys, value, binding)?;
                    }
                    Ok(None)
                }
                PropertyMutation::Unset => {
                    self.unset_resolved_place(register, &keys).map(|()| None)
                }
            },
            OverloadedMutation::Unresolved => match operation {
                PropertyMutation::Write(value, binding) => {
                    let value = value.into_value(self);
                    self.write_place(place, value, binding).map(|()| None)
                }
                PropertyMutation::Unset => self.unset_place(place).map(|()| None),
            },
            OverloadedMutation::Handled => {
                if let PropertyMutation::Write(mut value, _) = operation {
                    value.read(self);
                }
                Ok(None)
            }
            OverloadedMutation::Converted(value) => Ok(Some(value)),
            OverloadedMutation::Detached(root, keys) => {
                let (value, mutation) = match operation {
                    PropertyMutation::Write(value, binding) => {
                        (value.into_value(self), binding.into())
                    }
                    PropertyMutation::Unset => (Value::Null, Mutation::Unset),
                };
                self.write_detached_place(root, &keys, value, mutation)
                    .await
            }
        }
    }

    pub(crate) async fn write_detached_place(
        &mut self,
        mut root: Value,
        keys: &InlineStack<PathKey>,
        mut value: Value,
        mutation: Mutation,
    ) -> Result<Option<Value>, NativeError> {
        let mut converted = None;
        if mutation == Mutation::Assign
            && let Value::Object(object) = root.deref().clone()
            && let Some(PathKey::Property(key)) = keys.iter().next()
            && keys.iter().nth(1).is_none()
        {
            let object_value = value.is_object();
            value = self.property_string(&object, key, value).await?;
            if object_value && value.is_string() {
                converted = Some(value.clone());
            }
        }
        if self.validate_property_mutation(root.deref().clone(), keys, &mut value, mutation)? {
            return Ok(converted);
        }
        if mutation == Mutation::Assign {
            value = self.check_path_reference_assignment(&root, keys, value)?;
        }
        if mutation == Mutation::Unset
            && matches!(keys.iter().next(), Some(PathKey::Index(_)))
            && !root.is_array()
        {
            if root.is_null() {
                return Ok(None);
            }
            return Err(NativeError::error(
                "Cannot unset offset in a non-array variable",
            ));
        }
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        crate::path_update::update(
            &mut root,
            keys,
            value,
            mutation,
            &mut diagnostics.buffer,
            &diagnostics.vm.property_references,
        )?;
        Ok(converted)
    }

    async fn plan_magic_mutation(
        &mut self,
        place: &AccessPath,
        operation: &mut PropertyMutation,
    ) -> Result<OverloadedMutation, NativeError> {
        let assignment_local = match operation {
            PropertyMutation::Write(value, _) => value.pending_local(),
            PropertyMutation::Unset => None,
        };
        if place.steps()?.next().is_none() {
            return Ok(OverloadedMutation::Unresolved);
        }
        let creation = match operation {
            PropertyMutation::Write(..) => Creation::Create,
            PropertyMutation::Unset => Creation::Existing,
        };
        let Some(register) = self.base_register(place, creation, Missing::Silent)? else {
            return Ok(OverloadedMutation::Handled);
        };
        let mut cursor = self.read_root(register, Missing::Silent);
        let mut detached = None;
        let mut resolved = InlineStack::new();
        let mut steps = place.steps()?.peekable();
        while let Some(step) = steps.next() {
            let current = cursor.deref().clone();
            let key = match (step, &operation) {
                (Step::Index(index), PropertyMutation::Unset) => {
                    let Some(key) = self.unset_index_key(&current, index)? else {
                        return Ok(OverloadedMutation::Handled);
                    };
                    PathKey::Index(Some(key))
                }
                (Step::Index(index), _) => {
                    PathKey::Index(Some(self.write_index_key(&current, index)?))
                }
                _ => self.magic_path_key(step).await?,
            };
            if let (PathKey::Property(name), Value::Object(object)) = (&key, &current) {
                let class = self
                    .classes
                    .for_object(object)
                    .ok_or_else(|| internal_error!("property class"))?;
                let display = crate::path_update::property_name(name);
                let last = steps.peek().is_none();
                if let Some(access) = class.property_access {
                    use phera_extension_api::NativePropertyMutation as NativeMutation;
                    let change = match (last, &mut *operation) {
                        (false, _) => NativeMutation::Indirect,
                        (true, PropertyMutation::Write(value, Binding::ByValue)) => {
                            NativeMutation::Assign(value.read(self))
                        }
                        (true, PropertyMutation::Write(_, Binding::ByReference)) => {
                            NativeMutation::Bind
                        }
                        (true, PropertyMutation::Unset) => NativeMutation::Unset,
                    };
                    if (access.mutate)(self, object, &display, change)? {
                        return Ok(OverloadedMutation::Handled);
                    }
                }
                let (stored, slot) = self.property_slot(object, &display);
                if stored.is_none()
                    && class
                        .magic
                        .as_ref()
                        .is_some_and(|magic| magic.has_properties())
                    && !slot
                        .as_ref()
                        .is_some_and(|key| self.typed_property_uninitialized(&class, object, key))
                {
                    let (hook, argument) = match (last, &mut *operation) {
                        (true, PropertyMutation::Write(value, Binding::ByValue)) => {
                            (PropertyHook::Set, Some(value.read(self).clone()))
                        }
                        (true, PropertyMutation::Unset) => (PropertyHook::Unset, None),
                        _ => (PropertyHook::Get, None),
                    };
                    if let Some(method) = self.property_hook(&class, object, name, hook) {
                        let value = self
                            .invoke_property_hook(
                                class.clone(),
                                object.clone(),
                                hook,
                                name.clone(),
                                method,
                                argument,
                            )
                            .await?;
                        if hook != PropertyHook::Get {
                            if let Some(local) = assignment_local {
                                local.refresh_result(self);
                            }
                            return Ok(OverloadedMutation::Handled);
                        }
                        let value = self
                            .validate_getter_value(&class, slot.as_ref(), value)
                            .await?;
                        self.warn_indirect_magic_property(object, &display, &value);
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
                    None => self.property_key(object, &display)?,
                };
                if last
                    && let PropertyMutation::Write(value, Binding::ByValue) = operation
                    && {
                        let value = value.read(self);
                        self.property_needs_string(object, &key, value)
                    }
                {
                    detached = Some(current.clone());
                    resolved = InlineStack::new();
                }
                cursor = if matches!(steps.peek(), Some(Step::Index(_) | Step::Append)) {
                    let mutation = match operation {
                        PropertyMutation::Write(_, binding) => (*binding).into(),
                        PropertyMutation::Unset => Mutation::Unset,
                    };
                    self.prepare_property_dimension(object, &key, mutation)?
                } else {
                    stored.unwrap_or_default()
                };
                resolved.push(PathKey::Property(key));
            } else {
                cursor = match (&key, &current) {
                    (PathKey::Index(Some(key)), Value::Array(array)) => {
                        array.get(key).cloned().unwrap_or_default()
                    }
                    _ => Value::Null,
                };
                resolved.push(key);
            }
        }
        if let Value::Reference(reference) = cursor
            && matches!(resolved.iter().last(), Some(PathKey::Index(Some(_))))
            && let PropertyMutation::Write(value, Binding::ByValue) = operation
        {
            let value = value.read(self).clone();
            if self.may_cast_typed_reference(&value)
                && let Some(converted) =
                    self.assign_stringable_reference(&reference, &value).await?
            {
                return Ok(OverloadedMutation::Converted(converted));
            }
        }
        Ok(match detached {
            Some(value) => OverloadedMutation::Detached(value, Box::new(resolved)),
            None => OverloadedMutation::Ordinary(register, Box::new(resolved)),
        })
    }

    pub(crate) fn warn_indirect_magic_property(
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
}
