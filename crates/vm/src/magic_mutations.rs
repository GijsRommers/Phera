//! `$object->items['key'] = 1` writes through a getter's reference or object.
//! A getter returning an array by value gives a detached temporary.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::access_mode::{Binding, Creation, Missing, Mutation};
use crate::inline_stack::InlineStack;
use crate::magic_properties::PropertyHook;
use crate::path_update::PathKey;
use crate::{AccessPath, Vm};

pub(crate) enum PropertyMutation {
    Write(WriteValue, Binding),
    Unset,
}

pub(crate) struct WriteValue {
    value: Value,
    local: Option<AssignmentLocal>,
}

#[derive(Clone, Copy)]
struct AssignmentLocal {
    source: Reg,
    destination: Reg,
}

impl WriteValue {
    pub(crate) fn evaluated(value: Value) -> Self {
        Self { value, local: None }
    }

    pub(crate) fn local(source: Reg, destination: Reg) -> Self {
        Self {
            value: Value::Null,
            local: Some(AssignmentLocal {
                source,
                destination,
            }),
        }
    }

    /// Read a local only when the write reaches the final destination.
    fn read(&mut self, vm: &mut Vm) -> &Value {
        if let Some(local) = self.local.take() {
            self.value = vm.read_local(local.source);
            vm.set(local.destination, self.value.clone());
        }
        &self.value
    }

    fn into_value(mut self, vm: &mut Vm) -> Value {
        self.read(vm);
        self.value
    }
}

enum OverloadedMutation {
    Ordinary,
    Handled,
    Detached(Value, Box<InlineStack<PathKey>>),
}

impl Vm {
    pub(crate) async fn mutate_magic_place(
        &mut self,
        place: &AccessPath,
        mut operation: PropertyMutation,
    ) -> Result<(), NativeError> {
        let plan = self.plan_magic_mutation(place, &mut operation).await?;
        match plan {
            OverloadedMutation::Ordinary => match operation {
                PropertyMutation::Write(value, binding) => {
                    let value = value.into_value(self);
                    self.write_place(place, value, binding)
                }
                PropertyMutation::Unset => self.unset_place(place),
            },
            OverloadedMutation::Handled => {
                if let PropertyMutation::Write(mut value, _) = operation {
                    value.read(self);
                }
                Ok(())
            }
            OverloadedMutation::Detached(mut root, keys) => {
                let (mut value, mutation) = match operation {
                    PropertyMutation::Write(value, binding) => {
                        (value.into_value(self), binding.into())
                    }
                    PropertyMutation::Unset => (Value::Null, Mutation::Unset),
                };
                if mutation == Mutation::Assign
                    && let Value::Object(object) = root.deref().clone()
                    && let Some(PathKey::Property(key)) = keys.iter().next()
                    && keys.iter().nth(1).is_none()
                {
                    value = self.property_string(&object, key, value).await?;
                }
                if self.validate_property_mutation(
                    root.deref().clone(),
                    &keys,
                    &mut value,
                    mutation,
                )? {
                    return Ok(());
                }
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
        operation: &mut PropertyMutation,
    ) -> Result<OverloadedMutation, NativeError> {
        let assignment_local = match operation {
            PropertyMutation::Write(value, _) => value.local,
            PropertyMutation::Unset => None,
        };
        let Some(register) = self.base_register(place, Creation::Existing, Missing::Silent)? else {
            return Ok(OverloadedMutation::Ordinary);
        };
        let mut cursor = self.read_root(register, Missing::Silent);
        let mut detached = None;
        let mut resolved = InlineStack::new();
        let mut steps = place.steps()?.peekable();
        while let Some(step) = steps.next() {
            let (key, converted_name) = self.magic_path_key(step).await?;
            let current = cursor.deref().clone();
            if converted_name {
                detached = Some(cursor.clone());
                resolved = InlineStack::new();
            }
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
                                class,
                                object.clone(),
                                hook,
                                name.clone(),
                                method,
                                argument,
                            )
                            .await?;
                        if hook != PropertyHook::Get {
                            if let Some(local) = assignment_local {
                                // __set receives a copy, but the assignment result
                                // still comes from the caller's local after the call.
                                let value =
                                    self.read_defined_local(local.source).unwrap_or_default();
                                self.set(local.destination, value);
                            }
                            return Ok(OverloadedMutation::Handled);
                        }
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
                cursor = stored.unwrap_or_default();
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
        Ok(match detached {
            Some(value) => OverloadedMutation::Detached(value, Box::new(resolved)),
            None => OverloadedMutation::Ordinary,
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
