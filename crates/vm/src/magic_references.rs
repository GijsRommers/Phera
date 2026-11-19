//! A reference assignment such as `$ref =& $proxy->value` retains the cell
//! returned by &__get, or uses a detached temporary for a by-value getter.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::{Reg, Step};
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{Object, Value};

use crate::access_mode::{Creation, Missing, Mutation};
use crate::classes::property_hooks::HookRead;
use crate::code_table::FunctionIndex;
use crate::inline_stack::InlineStack;
use crate::magic_properties::{PropertyHook, PropertyRead, PropertyReadStep};
use crate::path_update::PathKey;
use crate::{AccessPath, Vm};

pub(crate) struct ReferencePath {
    place: AccessPath,
    register: Reg,
    next: usize,
    retry: Option<PathKey>,
    cursor: Value,
    detached: Option<Value>,
    keys: InlineStack<PathKey>,
    use_: ReferenceUse,
}

#[derive(Clone, Copy)]
pub(crate) enum ReferenceUse {
    Alias,
    Foreach,
}

pub(crate) enum ReferenceStep {
    Complete(Value),
    Lazy(Rc<Object>),
    Hook(FunctionIndex, ReferenceGetter),
}

pub(crate) struct ReferenceGetter {
    pub(crate) object: Rc<Object>,
    name: String,
    last: bool,
}

impl ReferencePath {
    pub(crate) fn resume(
        &mut self,
        getter: ReferenceGetter,
        value: Value,
    ) -> Result<Option<Value>, NativeError> {
        if !value.is_reference() && !value.is_object() {
            return Err(crate::classes::property_hooks::indirect_error(
                &getter.object,
                &getter.name,
            ));
        }
        Ok(self.retain_getter_value(value, getter.last))
    }

    fn retain_getter_value(&mut self, value: Value, last: bool) -> Option<Value> {
        if last {
            return Some(match value {
                Value::Reference(_) => value,
                _ => Value::Reference(Rc::new(RefCell::new(value))),
            });
        }
        self.cursor = value.clone();
        self.detached = Some(value);
        self.keys = InlineStack::new();
        None
    }
}

impl Vm {
    /// The alias returned by &__get points to the getter's own storage.
    pub(crate) fn start_magic_reference(
        &mut self,
        place: AccessPath,
        use_: ReferenceUse,
    ) -> Result<ReferencePath, NativeError> {
        let register = self
            .base_register(&place, Creation::Create, Missing::Silent)?
            .ok_or_else(|| internal_error!("missing reference place"))?;
        Ok(ReferencePath {
            place,
            register,
            next: 0,
            retry: None,
            cursor: self.raw_local(register),
            detached: None,
            keys: InlineStack::new(),
            use_,
        })
    }

    pub(crate) async fn advance_magic_reference(
        &mut self,
        path: &mut ReferencePath,
    ) -> Result<ReferenceStep, NativeError> {
        let place = path.place.clone();
        let mut steps = place.steps()?.skip(path.next).peekable();
        while let Some(step) = steps.next() {
            path.next += 1;
            let mut current = path.cursor.deref().clone();
            if let Step::Index(index) = step
                && let Value::Object(object) = &current
                && crate::dimensions::has_dimension(object)
            {
                return self
                    .reference_property_dimension(crate::dimensions::DimensionPath {
                        object: object.clone(),
                        offset: self.get(index),
                        suffix: steps.collect(),
                    })
                    .await
                    .map(ReferenceStep::Complete);
            }
            let key = if let Some(key) = path.retry.take() {
                key
            } else if let Step::Index(index) = step {
                PathKey::Index(Some(self.write_index_key(&current, index)?))
            } else {
                self.magic_path_key(step).await?
            };
            if let (PathKey::Property(name), Value::Object(object)) = (&key, &current) {
                let effective = self.lazy_property_receiver(
                    object,
                    name,
                    crate::lazy_property_access::LazyAccess::Reference,
                );
                if self.property_requires_initialization(
                    &effective,
                    name,
                    crate::lazy_property_access::LazyAccess::Reference,
                ) {
                    path.retry = Some(key);
                    path.next -= 1;
                    return Ok(ReferenceStep::Lazy(effective));
                }
                if !Rc::ptr_eq(object, &effective) {
                    if steps.peek().is_none()
                        && self.property_guarded(&effective, name, PropertyHook::Get)
                        && !effective.has(name)
                    {
                        let value = self.read_declared_property(
                            &effective,
                            &crate::path_update::property_name(name),
                            Missing::Warn,
                        )?;
                        return Ok(ReferenceStep::Complete(Value::Reference(Rc::new(
                            RefCell::new(value),
                        ))));
                    }
                    current = Value::Object(effective);
                    path.detached = Some(current.clone());
                    path.keys = InlineStack::new();
                }
            }
            let (key, next) = match (current, key) {
                (Value::Object(object), PathKey::Property(name)) => {
                    let display = crate::path_update::property_name(&name);
                    let (stored, slot) = self.property_slot(&object, &display);
                    if let Some(native) =
                        self.native_property_read(&object, &display, Missing::Silent)?
                    {
                        (PathKey::Property(slot.unwrap_or(name)), native)
                    } else {
                        let class = self
                            .classes
                            .for_object(&object)
                            .ok_or_else(|| internal_error!("property class"))?;
                        if let Some(property) = slot
                            .as_ref()
                            .and_then(|key| crate::classes::property_hooks::for_key(&class, key))
                            && let Some(hooks) = &property.hooks
                        {
                            let last = steps.peek().is_none();
                            match self.declared_hook_read(&object, property, hooks, true)? {
                                HookRead::Storage => {}
                                HookRead::Value(value) => {
                                    if let Some(value) = path.retain_getter_value(value, last) {
                                        return Ok(ReferenceStep::Complete(value));
                                    }
                                    continue;
                                }
                                HookRead::Getter(function) => {
                                    return Ok(ReferenceStep::Hook(
                                        function,
                                        ReferenceGetter {
                                            object,
                                            name: display,
                                            last,
                                        },
                                    ));
                                }
                            }
                        }
                        if object.is_uninitialized_lazy()
                            && steps.peek().is_none()
                            && stored.is_none()
                            && let Some(key) = &slot
                            && self.typed_property_uninitialized(&class, &object, key)
                            && let Some(property) =
                                class.defaults.iter().find(|property| property.key == *key)
                            && property.ty.as_ref().is_some_and(|ty| {
                                !ty.mask.intersects(phera_bytecode::TypeMask::NULL)
                            })
                        {
                            return Err(NativeError::error(format!(
                                "Cannot access uninitialized non-nullable property {}::${} by reference",
                                property.declaring, property.name,
                            )));
                        }
                        if stored.is_none()
                            && !slot.as_ref().is_some_and(|key| {
                                self.typed_property_uninitialized(&class, &object, key)
                            })
                            && self
                                .property_hook(&class, &object, &name, PropertyHook::Get)
                                .is_some()
                        {
                            let value = match self
                                .plan_property_read(
                                    Value::Object(object.clone()),
                                    name,
                                    PropertyRead::Read,
                                )
                                .await?
                            {
                                PropertyReadStep::Value(result) => result.value,
                                PropertyReadStep::Hook(function, object) => {
                                    return Ok(ReferenceStep::Hook(
                                        function,
                                        ReferenceGetter {
                                            object,
                                            name: display,
                                            last: steps.peek().is_none(),
                                        },
                                    ));
                                }
                            };
                            self.warn_indirect_magic_property(&object, &display, &value);
                            if let Some(value) =
                                path.retain_getter_value(value, steps.peek().is_none())
                            {
                                return Ok(ReferenceStep::Complete(value));
                            }
                            continue;
                        }
                        let key = match slot {
                            Some(key) => key,
                            None => self.property_key(&object, &display)?,
                        };
                        let value = if matches!(steps.peek(), Some(Step::Index(_) | Step::Append)) {
                            self.prepare_property_dimension(&object, &key, Mutation::Bind)?
                        } else {
                            stored.unwrap_or_default()
                        };
                        (PathKey::Property(key), value)
                    }
                }
                (Value::Array(array), PathKey::Index(Some(key))) => {
                    let value = array.get(&key).cloned().unwrap_or_default();
                    (PathKey::Index(Some(key)), value)
                }
                (Value::String(_), PathKey::Index(_)) => {
                    return Err(NativeError::error(
                        "Cannot create references to/from string offsets",
                    ));
                }
                (_, key) => (key, Value::Null),
            };
            path.cursor = next;
            path.keys.push(key);
        }
        // Foreach can bind the properties of a returned object without
        // replacing the slot that holds the object itself.
        if matches!(path.use_, ReferenceUse::Foreach)
            && let Value::Object(object) = &*path.cursor.deref()
        {
            return Ok(ReferenceStep::Complete(Value::Object(object.clone())));
        }
        let value = if let Some(root) = path.detached.take() {
            self.reference_from_magic(root, &path.keys, std::mem::take(&mut path.cursor))
        } else {
            let keys = std::mem::replace(&mut path.keys, InlineStack::new());
            self.reference_resolved(&place, path.register, keys)
        }?;
        Ok(ReferenceStep::Complete(value))
    }

    fn reference_from_magic(
        &mut self,
        mut root: Value,
        keys: &InlineStack<PathKey>,
        value: Value,
    ) -> NativeResult {
        if value.is_reference() {
            return Ok(value);
        }
        let mut reference = Value::Reference(Rc::new(RefCell::new(value)));
        self.validate_property_mutation(
            root.deref().clone(),
            keys,
            &mut reference,
            Mutation::Bind,
        )?;
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        crate::path_update::update(
            &mut root,
            keys,
            reference.clone(),
            Mutation::Bind,
            &mut diagnostics.buffer,
            &diagnostics.vm.property_references,
        )?;
        Ok(reference)
    }
}
