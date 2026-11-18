//! A reference assignment such as `$ref =& $proxy->value` retains the cell
//! returned by &__get, or uses a detached temporary for a by-value getter.

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::NativeResult;
use phera_value::Value;

use crate::access_mode::{Creation, Missing, Mutation};
use crate::inline_stack::InlineStack;
use crate::magic_properties::{PropertyHook, PropertyRead};
use crate::path_update::PathKey;
use crate::{AccessPath, Vm};

impl Vm {
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
                    if let Some(value) =
                        self.native_property_read(&object, &display, Missing::Silent)?
                    {
                        if steps.peek().is_none() {
                            return self.reference(place);
                        }
                        cursor = value;
                        continue;
                    }
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
