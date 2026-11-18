//! A cached `$item->name` can enter its getter in the same interpreter loop as
//! a normal method call. Plain property hits already returned before this path.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::calls::call_execution::CallEntry;
use crate::classes::property_hooks::HookWrite;
use crate::property_targets::{PropertyShape, PropertyTarget};
use crate::{AccessPath, Vm};

impl Vm {
    pub(crate) fn start_cached_getter(
        &mut self,
        place: &AccessPath,
        destination: Reg,
    ) -> Result<bool, NativeError> {
        let Some(PropertyTarget { object, name }) = self.read_property_target(place)? else {
            return Ok(false);
        };
        let Some(slot) = self
            .property_entry(&object, name)?
            .and_then(|entry| entry.hook_slot)
        else {
            return Ok(false);
        };
        let Some(class) = self.classes.for_object(&object) else {
            return Ok(false);
        };
        let Some(property) = class.defaults.get(slot) else {
            return Err(internal_error!("cached hooked property"));
        };
        let Some(hooks) = &property.hooks else {
            return Err(internal_error!("cached property hooks"));
        };
        let Some(getter) = &hooks.hooks.get else {
            return Ok(false);
        };
        let function = getter.function;
        if self.code.function(function).returns_reference
            || self.uses_property_backing(&object, property, hooks)
        {
            return Ok(false);
        }
        self.prepare_declared_hook(function, object, None)?;
        match self.enter_prepared_call(Some(destination))? {
            CallEntry::Php => Ok(true),
            _ => Err(internal_error!("prepared getter")),
        }
    }

    pub(crate) fn start_cached_setter(
        &mut self,
        place: &AccessPath,
        value: &Value,
    ) -> Result<bool, NativeError> {
        let Some(PropertyTarget { object, name }) =
            self.property_target(place, PropertyShape::Single)?
        else {
            return Ok(false);
        };
        let Some(slot) = self
            .property_entry(&object, name)?
            .and_then(|entry| entry.hook_slot)
        else {
            return Ok(false);
        };
        let Some(class) = self.classes.for_object(&object) else {
            return Ok(false);
        };
        let Some(property) = class.defaults.get(slot) else {
            return Err(internal_error!("cached hooked property"));
        };
        let Some(hooks) = &property.hooks else {
            return Err(internal_error!("cached property hooks"));
        };
        let HookWrite::Setter(function) = self.declared_hook_write(&object, property, hooks)?
        else {
            return Ok(false);
        };
        // Object-to-string coercion can call back into PHP before entering the setter.
        if value.is_object() {
            return Ok(false);
        }
        self.prepare_declared_hook(function, object, Some(value.clone()))?;
        match self.enter_prepared_call(None)? {
            CallEntry::Php => Ok(true),
            _ => Err(internal_error!("prepared setter")),
        }
    }
}
