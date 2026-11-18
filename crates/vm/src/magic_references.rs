//! A reference assignment such as `$ref =& $proxy->value` retains the cell
//! returned by &__get, or uses a detached temporary for a by-value getter.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::Step;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::Value;

use crate::access_mode::{Creation, Missing, Mutation};
use crate::inline_stack::InlineStack;
use crate::magic_properties::{PropertyHook, PropertyRead};
use crate::path_update::PathKey;
use crate::{AccessPath, Vm};

impl Vm {
    /// The alias returned by &__get points to the getter's own storage.
    pub(crate) async fn magic_reference(&mut self, place: &AccessPath) -> NativeResult {
        let register = self
            .base_register(place, Creation::Create, Missing::Silent)?
            .ok_or_else(|| internal_error!("missing reference place"))?;
        let mut cursor = self.raw_local(register);
        let mut detached = None;
        let mut keys = InlineStack::new();
        let mut steps = place.steps()?.peekable();
        while let Some(step) = steps.next() {
            let current = cursor.deref().clone();
            let key = if let Step::Index(index) = step {
                PathKey::Index(Some(self.write_index_key(&current, index)?))
            } else {
                self.magic_path_key(step).await?
            };
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
                        if stored.is_none()
                            && !slot.as_ref().is_some_and(|key| {
                                self.typed_property_uninitialized(&class, &object, key)
                            })
                            && self
                                .property_hook(&class, &object, &name, PropertyHook::Get)
                                .is_some()
                        {
                            let value = self
                                .read_overloaded_property(
                                    Value::Object(object.clone()),
                                    name,
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
                            cursor = value.clone();
                            detached = Some(value);
                            keys = InlineStack::new();
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
            cursor = next;
            keys.push(key);
        }
        if let Some(root) = detached {
            self.reference_from_magic(root, &keys, cursor)
        } else {
            self.reference_resolved(place, register, keys)
        }
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
