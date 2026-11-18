//! A cached `$item->name` can enter its getter in the same interpreter loop as
//! a normal method call. Plain property hits already returned before this path.

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{Object, Value};
use std::rc::Rc;

use crate::calls::call_execution::CallEntry;
use crate::code_table::FunctionIndex;
use crate::property_sites::PropertyEntry;
use crate::property_targets::PropertyTarget;
use crate::{AccessPath, Vm};

pub(crate) struct CachedPropertyHooks {
    pub(crate) getter: Option<FunctionIndex>,
    pub(crate) setter: Option<FunctionIndex>,
    pub(crate) guard_receiver: bool,
}

impl CachedPropertyHooks {
    fn dispatches(&self, vm: &Vm, object: &phera_value::Object) -> bool {
        !self.guard_receiver
            || !vm
                .current()
                .and_then(|frame| frame.this())
                .is_some_and(|receiver| std::ptr::eq(&**receiver, object))
    }
}

impl Vm {
    #[inline(never)]
    pub(crate) fn start_cached_getter(
        &mut self,
        place: &AccessPath,
        destination: Reg,
    ) -> Result<bool, NativeError> {
        let Some(PropertyTarget { object, name }) = self.read_property_target(place)? else {
            return Ok(false);
        };
        let Ok(Some(entry)) = self.property_entry(&object, name) else {
            return Ok(false);
        };
        self.enter_cached_getter(&entry, object, destination)
    }

    #[inline(never)]
    pub(crate) fn enter_cached_getter(
        &mut self,
        entry: &PropertyEntry,
        object: Rc<Object>,
        destination: Reg,
    ) -> Result<bool, NativeError> {
        let Some(function) = entry
            .hooks
            .as_deref()
            .filter(|hooks| hooks.dispatches(self, &object))
            .and_then(|hooks| hooks.getter)
        else {
            return Ok(false);
        };
        self.start_resolved_hook(function, object, None, Some(destination))?;
        Ok(true)
    }

    #[inline(never)]
    pub(crate) fn enter_cached_setter(
        &mut self,
        entry: &PropertyEntry,
        object: Rc<Object>,
        value: &Value,
        assignment: Option<(Reg, Reg)>,
    ) -> Result<bool, NativeError> {
        let Some(function) = entry
            .hooks
            .as_deref()
            .filter(|hooks| hooks.dispatches(self, &object))
            .and_then(|hooks| hooks.setter)
        else {
            return Ok(false);
        };
        // Object-to-string coercion can call back into PHP before entering the setter.
        if value.is_object() {
            return Ok(false);
        }
        if let Some((source, result)) = assignment {
            // A hook can change an aliased source before PHP copies the result.
            if self.raw_local(source).is_reference() {
                return Ok(false);
            }
            self.set(result, value.clone());
        }
        self.start_resolved_hook(function, object, Some(value), None)?;
        Ok(true)
    }

    fn start_resolved_hook(
        &mut self,
        function: FunctionIndex,
        object: Rc<Object>,
        value: Option<&Value>,
        destination: Option<Reg>,
    ) -> Result<(), NativeError> {
        if self.enter_exact_method(function, &object, value, destination)? {
            return Ok(());
        }
        self.prepare_declared_hook(function, object, value.cloned())?;
        match self.enter_prepared_call(destination)? {
            CallEntry::Php => Ok(()),
            _ => Err(internal_error!("prepared property hook")),
        }
    }
}
