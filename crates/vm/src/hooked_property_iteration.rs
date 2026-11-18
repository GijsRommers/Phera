//! `foreach ($object as $name => $value)` calls each visible getter as its
//! turn arrives. Array casts and debug inspection use the object's raw slots.

use std::{collections::HashSet, rc::Rc};

use phera_bytecode::{IterId, Reg, Target, Visibility};
use phera_extension_api::{NativeError, PropertyPurpose};
use phera_value::{ArrayKey, Object, Value};

use crate::Vm;
use crate::access_mode::Binding;
use crate::calls::frame::IteratorState;
use crate::classes::{
    Class,
    property_hooks::{HookRead, property_error},
};
use crate::code_table::FunctionIndex;
use crate::path_update::property_name;
use crate::property_hook_continuation::HookContinuation;

pub(crate) struct HookedPropertyIterator {
    pub(crate) object: Rc<Object>,
    class: Rc<Class>,
    next_declaration: usize,
    next_dynamic: Option<usize>,
    declared: HashSet<ArrayKey>,
    binding: Binding,
}

pub(crate) enum IterationStep {
    End,
    Entry(ArrayKey, Value),
    Getter(FunctionIndex, ArrayKey, String),
}

pub(crate) struct PropertyIteration {
    cursor: Box<HookedPropertyIterator>,
    iterator: IterId,
    key: Option<Reg>,
    destination: Reg,
}

pub(crate) struct IterationGetter {
    pub(crate) iteration: PropertyIteration,
    pub(crate) name: ArrayKey,
    pub(crate) property: String,
}

impl HookedPropertyIterator {
    pub(crate) fn new(object: Rc<Object>, class: Rc<Class>, binding: Binding) -> Self {
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

    pub(crate) fn next(
        &mut self,
        vm: &mut Vm,
        purpose: PropertyPurpose,
    ) -> Result<IterationStep, NativeError> {
        let by_reference = self.binding == Binding::ByReference;
        let public_only = purpose == PropertyPurpose::Json;
        let scoped = matches!(
            purpose,
            PropertyPurpose::Foreach | PropertyPurpose::ObjectVars
        );
        while let Some(property) = self.class.defaults.get(self.next_declaration) {
            self.next_declaration += 1;
            if public_only && property.visibility != Visibility::Public {
                continue;
            }
            if scoped
                && !vm
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
                match vm.declared_hook_read(&self.object, property, hooks, by_reference)? {
                    HookRead::Storage => {}
                    HookRead::Value(value) => {
                        return Ok(IterationStep::Entry(
                            enumeration_key(purpose, property),
                            value,
                        ));
                    }
                    HookRead::Getter(function) => {
                        return Ok(IterationStep::Getter(
                            function,
                            enumeration_key(purpose, property),
                            property.name.clone(),
                        ));
                    }
                }
            }
            let stored = self.object.get(&property.key).or_else(|| {
                (purpose != PropertyPurpose::Foreach
                    && property
                        .hooks
                        .as_ref()
                        .is_some_and(|hooks| hooks.hooks.set.is_some()))
                .then_some(Value::Null)
            });
            if let Some(value) = stored {
                let value = if by_reference {
                    vm.object_iteration_cell(&self.object, &property.key, value)?
                } else {
                    value
                };
                return Ok(IterationStep::Entry(
                    enumeration_key(purpose, property),
                    value,
                ));
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
                && (!scoped
                    || vm
                        .property_key(&self.object, &property_name(&key))
                        .is_ok_and(|resolved| resolved == key))
            {
                let value = if by_reference {
                    vm.object_iteration_cell(&self.object, &key, value.clone())?
                } else {
                    value.clone()
                };
                return Ok(IterationStep::Entry(key, value));
            }
            position = properties.next_position(index);
        }
        Ok(IterationStep::End)
    }
}

fn enumeration_key(
    purpose: PropertyPurpose,
    property: &crate::classes::class::PropertyDefault,
) -> ArrayKey {
    if purpose == PropertyPurpose::Export {
        property.key.clone()
    } else {
        ArrayKey::str(property.name.as_str())
    }
}

impl Vm {
    pub(crate) fn hooked_property_class(&self, object: &Object) -> Option<Rc<Class>> {
        self.classes.for_object(object).filter(|class| {
            class.defaults.iter().any(|property| {
                property
                    .hooks
                    .as_ref()
                    .is_some_and(|hooks| hooks.has_hooks())
            })
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
        let entry = cursor.next(self, PropertyPurpose::Foreach)?;
        let iteration = PropertyIteration {
            cursor,
            iterator,
            key,
            destination,
        };
        match entry {
            IterationStep::End => self.jump(end)?,
            IterationStep::Entry(name, value) => {
                self.finish_hooked_iteration(iteration, name, value)
            }
            IterationStep::Getter(function, name, property) => {
                let receiver = iteration.cursor.object.clone();
                self.suspend_for_hook(
                    function,
                    receiver,
                    None,
                    HookContinuation::Iteration(IterationGetter {
                        iteration,
                        name,
                        property,
                    }),
                )
                .await?;
            }
        }
        Ok(true)
    }

    pub(crate) fn resume_hooked_iteration(
        &mut self,
        getter: IterationGetter,
        value: Value,
    ) -> Result<(), NativeError> {
        if getter.iteration.cursor.binding == Binding::ByReference
            && !value.is_reference()
            && !value.is_object()
        {
            return Err(crate::classes::property_hooks::indirect_error(
                &getter.iteration.cursor.object,
                &getter.property,
            ));
        }
        self.finish_hooked_iteration(getter.iteration, getter.name, value);
        Ok(())
    }

    fn finish_hooked_iteration(
        &mut self,
        iteration: PropertyIteration,
        name: ArrayKey,
        value: Value,
    ) {
        let PropertyIteration {
            cursor,
            iterator,
            key,
            destination,
        } = iteration;
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
    }
}
