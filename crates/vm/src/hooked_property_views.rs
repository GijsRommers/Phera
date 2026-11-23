//! `get_object_vars($row)` reads visible getters in declaration order.
//! Array casts and debug views can instead use the unmodified backing slots.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, PropertyPurpose};
use phera_value::{ArrayKey, Object, PhpArray, Value};

use crate::Vm;
use crate::access_mode::Binding;
use crate::classes::Class;
use crate::error_traces::NativeTrace;
use crate::hooked_property_iteration::{HookedPropertyIterator, IterationStep};
use crate::property_hook_continuation::HookContinuation;

pub(crate) struct ObjectVars {
    cursor: HookedPropertyIterator,
    properties: PhpArray,
    destination: Reg,
    pub(crate) native: NativeTrace,
}

impl Vm {
    pub(crate) async fn suspend_object_vars(
        &mut self,
        object: Rc<Object>,
        class: Rc<Class>,
        destination: Reg,
    ) -> Result<(), NativeError> {
        let native = NativeTrace::new("get_object_vars", &[Value::Object(object.clone())]);
        self.continue_object_vars(ObjectVars {
            cursor: HookedPropertyIterator::new(object, class, Binding::ByValue),
            properties: PhpArray::new(),
            destination,
            native,
        })
        .await
    }

    pub(crate) async fn resume_object_vars(
        &mut self,
        mut view: ObjectVars,
        key: ArrayKey,
        value: Value,
    ) -> Result<(), NativeError> {
        view.properties.insert(key, value);
        self.continue_object_vars(view).await
    }

    async fn continue_object_vars(&mut self, mut view: ObjectVars) -> Result<(), NativeError> {
        loop {
            match view.cursor.next(self, PropertyPurpose::ObjectVars)? {
                IterationStep::End => {
                    self.set(view.destination, Value::from(view.properties));
                    return Ok(());
                }
                IterationStep::Entry(key, value) => {
                    view.properties.insert(key, value);
                }
                IterationStep::Getter(function, key, _) => {
                    return self
                        .suspend_for_hook(
                            function,
                            view.cursor.object.clone(),
                            None,
                            HookContinuation::ObjectVars(view, key),
                        )
                        .await;
                }
            }
        }
    }

    pub(crate) async fn enumerate_hooked_native(
        &mut self,
        object: Rc<Object>,
        class: Rc<Class>,
        purpose: PropertyPurpose,
        native: crate::error_traces::NativeTrace,
    ) -> Result<PhpArray, NativeError> {
        self.native_callback_sites
            .push(crate::error_traces::NativeCallbackSite {
                boundary: self.call_stack.depth(),
                native: crate::error_traces::NativeCaller::Saved(native),
            });
        let result = self
            .enumerate_hooked_properties(object, class, purpose)
            .await;
        self.native_callback_sites.pop();
        result
    }

    async fn enumerate_hooked_properties(
        &mut self,
        object: Rc<Object>,
        class: Rc<Class>,
        purpose: PropertyPurpose,
    ) -> Result<PhpArray, NativeError> {
        let mut cursor = HookedPropertyIterator::new(object, class, Binding::ByValue);
        let mut properties = PhpArray::new();
        loop {
            match cursor.next(self, purpose)? {
                IterationStep::End => return Ok(properties),
                IterationStep::Entry(key, value) => properties.insert(key, value),
                IterationStep::Getter(function, key, _) => {
                    let value = self
                        .invoke_declared_hook(function, cursor.object.clone(), None)
                        .await?;
                    properties.insert(key, value)
                }
            };
        }
    }
}

impl Vm {
    pub(crate) fn raw_hooked_slots(&self, object: &Object, class: &Class) -> PhpArray {
        let mut slots = PhpArray::new();
        for property in &class.defaults {
            if let Some(value) = object.get(&property.key) {
                slots.insert(property.key.clone(), value);
            }
        }
        for (key, value) in object.to_array().iter() {
            if !slots.contains_key(&key) {
                slots.insert(key, value.clone());
            }
        }
        slots
    }

    pub(crate) fn dump_hooked_slots(
        &self,
        object: &Object,
        class: &Class,
    ) -> phera_extension_api::PropertyView {
        let mut slots = PhpArray::new();
        // PhpString's cached hash cell does not change Eq or Hash.
        #[allow(clippy::mutable_key_type)]
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
