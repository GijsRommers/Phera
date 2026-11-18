//! A retained property receiver can be written after a compound read or clone.
//! `$proxy->child->count++` keeps the object returned by `__get('child')`.

use std::rc::Rc;

use phera_extension_api::{NativeError, NativePropertyMutation};
use phera_value::{ArrayKey, Object, Value};

use crate::Vm;
use crate::access_mode::Mutation;
use crate::inline_stack::InlineStack;
use crate::magic_properties::PropertyHook;
use crate::path_update::{PathKey, property_name};

impl Vm {
    pub(crate) async fn assign_selected_property(
        &mut self,
        object: Rc<Object>,
        name: ArrayKey,
        value: Value,
    ) -> Result<(), NativeError> {
        let class = self
            .classes
            .for_object(&object)
            .ok_or_else(|| internal_error!("property class"))?;
        let display = property_name(&name);
        if let Some(access) = class.property_access
            && (access.mutate)(
                self,
                &object,
                &display,
                NativePropertyMutation::Assign(&value),
            )?
        {
            return Ok(());
        }
        let (stored, slot) = self.property_slot(&object, &display);
        if stored.is_none()
            && !slot
                .as_ref()
                .is_some_and(|slot| self.typed_property_uninitialized(&class, &object, slot))
            && let Some(method) = self.property_hook(&class, &object, &name, PropertyHook::Set)
        {
            self.invoke_property_hook(class, object, PropertyHook::Set, name, method, Some(value))
                .await?;
            return Ok(());
        }
        let key = match slot {
            Some(key) => key,
            None => self.property_key(&object, &display)?,
        };
        let mut keys = InlineStack::new();
        keys.push(PathKey::Property(key));
        self.write_detached_place(Value::Object(object), &keys, value, Mutation::Assign)
            .await
    }
}
