//! An include at file scope can create `$x` for the rest of the request.
//! `$x`, `$GLOBALS['x']` and a function's `global $x` use the same symbol.

use std::{cell::RefCell, rc::Rc};

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, PhpArray, Value};

use crate::{
    Vm,
    access_mode::{Creation, Missing},
    calls::frame::FrameView,
};

impl Vm {
    /// The innermost consecutive file-scope frame owns the active register window.
    pub(crate) fn global_frame(&self) -> Option<FrameView<'_>> {
        self.frames()
            .take_while(|frame| frame.function_id().is_none())
            .last()
    }

    pub(crate) fn in_global_scope(&self) -> bool {
        self.current()
            .zip(self.global_frame())
            .is_some_and(|(current, global)| current.base() == global.base())
    }

    fn global_slot(&self, name: &str) -> Option<(usize, Reg)> {
        let frame = self.global_frame()?;
        Some((frame.base(), frame.named_register(name)?))
    }

    /// Symbols without a register live in the request table.
    pub(crate) fn request_global_value(&self, name: &str) -> Option<&Value> {
        if let Some((base, register)) = self.global_slot(name) {
            self.registers
                .window(base + register.index(), 1)
                .first()?
                .as_ref()
        } else {
            self.globals.get(name)
        }
    }

    pub(crate) fn take_request_global(&mut self, name: &str) -> Option<Value> {
        if let Some((base, register)) = self.global_slot(name) {
            self.registers
                .window_mut(base + register.index(), 1)
                .first_mut()?
                .take()
        } else {
            self.globals.take(name)
        }
    }

    pub(crate) fn store_request_global(&mut self, name: String, value: Option<Value>) {
        if let Some((base, register)) = self.global_slot(&name) {
            self.registers.window_mut(base + register.index(), 1)[0] = value;
        } else if let Some(value) = value {
            self.globals.insert(name, value);
        } else {
            self.globals.remove(&name);
        }
    }

    pub(crate) fn global_reference(&mut self, name: String) -> Value {
        if let Some((base, register)) = self.global_slot(&name) {
            let slot = &mut self.registers.window_mut(base + register.index(), 1)[0];
            let value = slot.get_or_insert(Value::Null);
            if !value.is_reference() {
                *value = Value::Reference(Rc::new(RefCell::new(std::mem::take(value))));
            }
            value.clone()
        } else {
            self.globals.reference(name)
        }
    }

    /// A nonlocal path keeps the cell in a scratch register during the operation.
    pub(crate) fn global_register(
        &mut self,
        name: String,
        scratch: Reg,
        creation: Creation,
        missing: Missing,
    ) -> Result<Option<Reg>, NativeError> {
        if self.request_global_value(&name).is_none() && creation == Creation::Existing {
            if missing == Missing::Warn {
                self.warn(format!("Undefined global variable ${name}"));
            }
            return Ok(None);
        }
        if let Some((base, register)) = self.global_slot(&name)
            && self.current().is_some_and(|frame| frame.base() == base)
        {
            return Ok(Some(register));
        }
        let reference = self.global_reference(name);
        self.set(scratch, reference);
        Ok(Some(scratch))
    }

    /// File-scope registers give ownership back to the request table while parked.
    pub(crate) fn park_global_registers(&mut self) {
        for (name, register) in self.global_register_names() {
            if let Some(value) = self.registers.window_mut(register, 1)[0].take() {
                self.globals.insert(name, value);
            }
        }
    }

    pub(crate) fn restore_global_registers(&mut self) {
        for (name, register) in self.global_register_names() {
            self.registers.window_mut(register, 1)[0] = self.globals.take(&name);
        }
    }

    fn global_register_names(&self) -> Vec<(String, usize)> {
        self.global_frame().map_or_else(Vec::new, |frame| {
            frame
                .named_registers()
                .map(|(name, register)| (name.clone(), frame.base() + register.index()))
                .collect()
        })
    }

    /// Reading the whole table copies values rather than exposing its writable backing cells.
    pub(crate) fn global_snapshot(&self) -> NativeResult {
        let mut values = PhpArray::new();
        for (name, value) in self.globals.iter() {
            values.insert(ArrayKey::str(name.as_str()), self.copy_global_value(value));
        }
        if let Some(frame) = self.global_frame() {
            for (name, register) in frame.named_registers() {
                if let Some(Some(value)) = frame.slots(&self.registers).get(register.index()) {
                    values.insert(ArrayKey::str(name.as_str()), self.copy_global_value(value));
                }
            }
        }
        Ok(Value::Array(Rc::new(values)))
    }

    /// Scratch registers exist only for evaluation and do not make a PHP alias.
    fn copy_global_value(&self, value: &Value) -> Value {
        let Value::Reference(reference) = value else {
            return value.clone();
        };
        let mut internal_owners = 1;
        for frame in self.frames() {
            for (index, slot) in frame
                .slots(&self.registers)
                .iter()
                .enumerate()
                .skip(frame.function().locals.len())
            {
                if !frame
                    .extras
                    .dynamic
                    .iter()
                    .any(|(_, register)| register.index() == index)
                    && matches!(slot, Some(Value::Reference(cell)) if Rc::ptr_eq(cell, reference))
                {
                    internal_owners += 1;
                }
            }
        }
        if Rc::strong_count(reference) > internal_owners {
            value.clone()
        } else {
            value.deref().clone()
        }
    }

    pub(crate) fn publish_global(&mut self, name: String, value: Value) {
        self.store_request_global(name, Some(value));
    }

    pub(crate) fn remove_global(&mut self, name: &str) {
        self.take_request_global(name);
    }
}
