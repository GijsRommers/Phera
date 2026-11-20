//! `$object->items['key'] = 1` writes through a getter's reference or object.
//! A getter returning an array by value gives a detached temporary.

use phera_bytecode::{Reg, Step};
use phera_extension_api::NativeError;
use phera_value::{Object, Value};
use std::rc::Rc;

use crate::access_mode::{Binding, Mutation};
use crate::classes::property_hooks::{HookRead, HookWrite};
use crate::code_table::FunctionIndex;
use crate::inline_stack::InlineStack;
use crate::magic_properties::PropertyHook;
use crate::path_update::PathKey;
use crate::{AccessPath, Vm};

mod execution;
mod write_value;
pub(crate) use write_value::{AssignmentLocal, WriteValue};

pub(crate) enum PropertyMutation {
    Write(WriteValue, Binding),
    Unset,
}

enum OverloadedMutation {
    Lazy(Rc<Object>),
    Ordinary(Reg, Box<InlineStack<PathKey>>),
    Unresolved,
    Handled,
    Converted(Value),
    Dimension(crate::dimensions::DimensionPath),
    Detached(Value, Box<InlineStack<PathKey>>),
    Hook(MutationHook),
}

pub(crate) enum MutationHook {
    Getter {
        function: FunctionIndex,
        object: Rc<Object>,
        name: String,
        last: bool,
        indirect: bool,
    },
    Setter {
        function: FunctionIndex,
        object: Rc<Object>,
        value: Value,
        local: Option<AssignmentLocal>,
    },
}

pub(crate) enum MutationStep {
    Lazy(Rc<Object>),
    Complete(Option<Value>),
    Hook(MutationHook),
}

pub(crate) struct MutationPath {
    place: AccessPath,
    operation: PropertyMutation,
    register: Option<Reg>,
    cursor: Value,
    detached: Option<Value>,
    resolved: InlineStack<PathKey>,
    next: usize,
    retry: Option<PathKey>,
}

impl Vm {
    async fn plan_magic_mutation(
        &mut self,
        path: &mut MutationPath,
    ) -> Result<OverloadedMutation, NativeError> {
        let operation = &mut path.operation;
        let assignment_local = match operation {
            PropertyMutation::Write(value, _) => value.pending_local(),
            PropertyMutation::Unset => None,
        };
        if path.place.steps()?.next().is_none() {
            return Ok(OverloadedMutation::Unresolved);
        }
        let Some(register) = path.register else {
            return Ok(OverloadedMutation::Handled);
        };
        let mut cursor = std::mem::take(&mut path.cursor);
        let mut steps = path.place.steps()?.skip(path.next).peekable();
        while let Some(step) = steps.next() {
            path.next += 1;
            let mut current = cursor.deref().clone();
            if matches!(step, Step::Index(_) | Step::Append)
                && let Value::Object(object) = &current
                && crate::dimensions::has_dimension(object)
            {
                let offset = match step {
                    Step::Index(index) => self.get(index),
                    _ => Value::Null,
                };
                return Ok(OverloadedMutation::Dimension(
                    crate::dimensions::DimensionPath {
                        object: object.clone(),
                        offset,
                        suffix: steps.collect(),
                    },
                ));
            }
            let key = if let Some(key) = path.retry.take() {
                key
            } else {
                match (step, &operation) {
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
                }
            };
            if let (PathKey::Property(name), Value::Object(object)) = (&key, &current) {
                let access = match (steps.peek().is_none(), &operation) {
                    (true, PropertyMutation::Write(_, Binding::ByValue)) => {
                        crate::lazy_property_access::LazyAccess::Write
                    }
                    (true, PropertyMutation::Unset) => {
                        crate::lazy_property_access::LazyAccess::Unset
                    }
                    _ => crate::lazy_property_access::LazyAccess::Reference,
                };
                let effective = self.lazy_property_receiver(object, name, access);
                if self.property_requires_initialization(&effective, name, access) {
                    path.cursor = cursor;
                    path.retry = Some(key);
                    path.next -= 1;
                    return Ok(OverloadedMutation::Lazy(effective));
                }
                if !Rc::ptr_eq(object, &effective) {
                    current = Value::Object(effective);
                    path.detached = Some(current.clone());
                    path.resolved = InlineStack::new();
                }
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
                    if !last
                        && let Some(value) = (access.read)(
                            self,
                            object,
                            &display,
                            phera_extension_api::NativePropertyRead::Update,
                        )?
                        && value.is_object()
                    {
                        // Object-valued native getters return a handle. The
                        // remaining write is to that object, not the getter.
                        cursor = value.clone();
                        path.detached = Some(value);
                        path.resolved = InlineStack::new();
                        continue;
                    }
                    let converted;
                    let change = match (last, &mut *operation) {
                        (false, _) => NativeMutation::Indirect,
                        (true, PropertyMutation::Write(value, Binding::ByValue)) => {
                            let value = value.read(self);
                            converted = self
                                .native_string_assignment(access, object, &display, value)
                                .await?;
                            NativeMutation::Assign(converted.as_ref().unwrap_or(value))
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
                let declared = slot
                    .as_ref()
                    .and_then(|key| crate::classes::property_hooks::for_key(&class, key));
                if let Some(property) = declared
                    && let Some(hooks) = &property.hooks
                {
                    use crate::classes::property_hooks::property_error;
                    if last && let PropertyMutation::Write(value, Binding::ByValue) = operation {
                        if let HookWrite::Setter(setter) =
                            self.declared_hook_write(object, property, hooks)?
                        {
                            return Ok(OverloadedMutation::Hook(MutationHook::Setter {
                                function: setter,
                                object: object.clone(),
                                value: value.read(self).clone(),
                                local: assignment_local,
                            }));
                        }
                    } else if last
                        && matches!(operation, PropertyMutation::Unset)
                        && hooks.has_hooks()
                    {
                        return Err(property_error(
                            object,
                            &display,
                            "Cannot unset hooked property ",
                            "",
                        ));
                    } else if last && matches!(operation, PropertyMutation::Unset) {
                        self.check_set_visibility(
                            property,
                            crate::property_set_visibility::PropertyChange::Unset,
                        )?;
                    } else {
                        let indirect = !last || hooks.hooks.get.is_none();
                        match self.declared_hook_read(object, property, hooks, indirect)? {
                            HookRead::Storage => {}
                            HookRead::Getter(function) => {
                                return Ok(OverloadedMutation::Hook(MutationHook::Getter {
                                    function,
                                    object: object.clone(),
                                    name: display,
                                    last,
                                    indirect,
                                }));
                            }
                            HookRead::Value(value) => {
                                check_magic_write_hook_last(last)?;
                                cursor = value.clone();
                                path.detached = Some(value);
                                path.resolved = InlineStack::new();
                                continue;
                            }
                        }
                    }
                }
                if declared.is_none()
                    && stored.is_none()
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
                        path.detached = Some(value);
                        path.resolved = InlineStack::new();
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
                    path.detached = Some(current.clone());
                    path.resolved = InlineStack::new();
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
                path.resolved.push(PathKey::Property(key));
            } else {
                cursor = match (&key, &current) {
                    (PathKey::Index(Some(key)), Value::Array(array)) => {
                        array.get(key).cloned().unwrap_or_default()
                    }
                    _ => Value::Null,
                };
                path.resolved.push(key);
            }
        }
        if let Value::Reference(reference) = cursor
            && matches!(path.resolved.iter().last(), Some(PathKey::Index(Some(_))))
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
        let resolved = Box::new(std::mem::replace(&mut path.resolved, InlineStack::new()));
        Ok(match path.detached.take() {
            Some(value) => OverloadedMutation::Detached(value, resolved),
            None => OverloadedMutation::Ordinary(register, resolved),
        })
    }

    pub(crate) fn resume_magic_mutation(
        &mut self,
        path: &mut MutationPath,
        value: Value,
        object: &Object,
        name: &str,
        last: bool,
        indirect: bool,
    ) -> Result<(), NativeError> {
        if indirect && !value.is_reference() && !value.is_object() {
            return Err(crate::classes::property_hooks::indirect_error(object, name));
        }
        check_magic_write_hook_last(last)?;
        path.cursor = value.clone();
        path.detached = Some(value);
        path.resolved = InlineStack::new();
        Ok(())
    }
}

fn check_magic_write_hook_last(last: bool) -> Result<(), NativeError> {
    if last {
        return Err(NativeError::error(
            "Cannot assign by reference to overloaded object",
        ));
    }
    Ok(())
}
