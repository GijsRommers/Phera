//! `foreach ($object as $name => $value)` calls each visible getter as its
//! turn arrives. Array casts and debug inspection use the object's raw slots.

use std::{collections::HashSet, rc::Rc};

use phera_bytecode::{IterId, Reg, Target, Visibility};
use phera_extension_api::{NativeError, PropertyPurpose};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use crate::Vm;
use crate::access_mode::Binding;
use crate::calls::frame::IteratorState;
use crate::classes::{Class, property_hooks::property_error};

pub(crate) struct HookedPropertyIterator {
    object: Rc<Object>,
    class: Rc<Class>,
    next_declaration: usize,
    next_dynamic: Option<usize>,
    declared: HashSet<ArrayKey>,
    binding: Binding,
}

impl HookedPropertyIterator {
    fn new(object: Rc<Object>, class: Rc<Class>, binding: Binding) -> Self {
        let declared = class
            .defaults
            .iter()
            .map(|property| property.key.clone())
            .collect();
        Self {
            object,
            class,
            binding,
            declared,
            next_declaration: 0,
            next_dynamic: None,
        }
    }

    async fn next(
        &mut self,
        vm: &mut Vm,
        purpose: PropertyPurpose,
    ) -> Result<Option<(ArrayKey, Value)>, NativeError> {
        let by_reference = self.binding == Binding::ByReference;
        let public_only = purpose == PropertyPurpose::Json;
        while let Some(property) = self.class.defaults.get(self.next_declaration) {
            self.next_declaration += 1;
            if public_only && property.visibility != Visibility::Public {
                continue;
            }
            if !vm
                .property_key(&self.object, &property.name)
                .is_ok_and(|key| key == property.key)
            {
                continue;
            }
            if let Some(hooks) = &property.hooks {
                if hooks.hooks.is_virtual && hooks.hooks.get.is_none() {
                    continue;
                }
                if by_reference && hooks.has_hooks() {
                    let can_reference =
                        hooks.hooks.get.as_ref().is_some_and(|getter| {
                            vm.code.function(getter.function).returns_reference
                        });
                    if !can_reference {
                        return Err(property_error(
                            &self.object,
                            &property.name,
                            "Cannot create reference to property ",
                            "",
                        ));
                    }
                }
                if let Some(value) = vm
                    .read_declared_hook(&self.object, property, hooks, by_reference)
                    .await?
                {
                    return Ok(Some((ArrayKey::str(property.name.as_str()), value)));
                }
            }
            if let Some(value) = self.object.get(&property.key) {
                let value = if by_reference {
                    vm.object_iteration_cell(&self.object, &property.key, value)?
                } else {
                    value
                };
                return Ok(Some((ArrayKey::str(property.name.as_str()), value)));
            }
        }
        let properties = self.object.to_array();
        let mut position = self.next_dynamic.map_or_else(
            || properties.first_position(),
            |last| properties.next_position(last),
        );
        while let Some(index) = position {
            self.next_dynamic = Some(index);
            if let Some((key, value)) = properties.entry_at(index)
                && !self.declared.contains(&key)
            {
                let value = if by_reference {
                    vm.object_iteration_cell(&self.object, &key, value.clone())?
                } else {
                    value.clone()
                };
                return Ok(Some((key, value)));
            }
            position = properties.next_position(index);
        }
        Ok(None)
    }
}

impl Vm {
    pub(crate) fn hooked_property_class(&self, object: &Object) -> Option<Rc<Class>> {
        self.classes.for_object(object).filter(|class| {
            class
                .defaults
                .iter()
                .any(|property| property.hooks.is_some())
        })
    }

    pub(crate) fn start_hooked_property_iteration(
        &mut self,
        iterator: IterId,
        object: Rc<Object>,
        class: Rc<Class>,
        binding: Binding,
    ) {
        self.call_stack.current_extras_mut().iterators.insert(
            iterator.0,
            IteratorState {
                snapshot: None,
                traversal: None,
                property_hooks: Some(Box::new(HookedPropertyIterator::new(
                    object, class, binding,
                ))),
                source: None,
                position: None,
                started: false,
            },
        );
    }

    pub(crate) async fn next_hooked_property(
        &mut self,
        iterator: IterId,
        key: Option<Reg>,
        destination: Reg,
        end: Target,
    ) -> Result<bool, NativeError> {
        let iterator_state = self
            .call_stack
            .current_extras_mut()
            .iterators
            .get_mut(iterator.0);
        let Some(mut cursor) = iterator_state.and_then(|state| state.property_hooks.take()) else {
            return Ok(false);
        };
        let entry = cursor.next(self, PropertyPurpose::Foreach).await?;
        let Some((name, value)) = entry else {
            return self.jump(end).map(|()| true);
        };
        if let Some(key) = key {
            let name = match name {
                ArrayKey::String(name) => Value::String(name),
                ArrayKey::Int(index) => Value::Int(index),
            };
            self.set(key, name);
        }
        let value = if cursor.binding == Binding::ByReference {
            value
        } else {
            value.deref().clone()
        };
        let previous = self.raw_local(destination);
        self.set(destination, value);
        if let Value::Reference(reference) = previous {
            self.release_property_reference(reference);
        }
        if let Some(state) = self
            .call_stack
            .current_extras_mut()
            .iterators
            .get_mut(iterator.0)
        {
            state.property_hooks = Some(cursor);
        }
        Ok(true)
    }

    pub(crate) async fn enumerate_hooked_properties(
        &mut self,
        object: Rc<Object>,
        class: Rc<Class>,
        purpose: PropertyPurpose,
    ) -> Result<PhpArray, NativeError> {
        let mut cursor = HookedPropertyIterator::new(object, class, Binding::ByValue);
        let mut properties = PhpArray::new();
        while let Some((key, value)) = cursor.next(self, purpose).await? {
            properties.insert(key, value);
        }
        Ok(properties)
    }
}

impl Vm {
    pub(crate) fn dump_hooked_slots(
        &self,
        object: &Object,
        class: &Class,
    ) -> phera_extension_api::PropertyView {
        let mut slots = PhpArray::new();
        let mut uninitialized = std::collections::HashMap::new();
        for property in &class.defaults {
            if property
                .hooks
                .as_ref()
                .is_some_and(|hooks| hooks.hooks.is_virtual)
            {
                continue;
            }
            if let Some(value) = object.get(&property.key) {
                slots.insert(property.key.clone(), value);
            } else if let Some(ty) = &property.ty {
                slots.insert(property.key.clone(), Value::Null);
                uninitialized.insert(property.key.clone(), ty.display.clone());
            }
        }
        for (key, value) in object.to_array().iter() {
            if !slots.contains_key(&key) {
                slots.insert(key, value.clone());
            }
        }
        phera_extension_api::PropertyView::DebugSlots {
            slots: Rc::new(slots),
            uninitialized,
        }
    }
}
