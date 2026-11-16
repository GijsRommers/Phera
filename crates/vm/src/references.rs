//! Global and static reference identity.
use crate::Vm;
use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;
use std::{cell::RefCell, rc::Rc, sync::Arc};
/// Static storage is scoped by compiled unit, function, and static variable.
#[derive(Hash, PartialEq, Eq)]
pub(crate) struct StaticKey {
    pub(crate) unit: usize,
    pub(crate) function: Option<usize>,
    pub(crate) variable: u16,
}

impl Vm {
    pub(crate) fn name(&self, name: phera_bytecode::NameId) -> Result<String, NativeError> {
        self.current()
            .and_then(|frame| frame.unit.names.get(name.index()))
            .cloned()
            .ok_or_else(|| NativeError::error("Phera internal error: invalid name"))
    }

    pub(crate) fn static_key(
        &self,
        variable: phera_bytecode::StaticId,
    ) -> Result<StaticKey, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| NativeError::error("Phera internal error: static"))?;
        Ok(StaticKey {
            unit: Arc::as_ptr(&frame.unit) as usize,
            function: frame.function_id,
            variable: variable.0,
        })
    }

    pub(crate) fn bind_global(&mut self, local: Reg, name: String) -> Result<(), NativeError> {
        if let Some(frame) = self.call_stack.frames.first_mut()
            && let Some(index) = frame
                .function()
                .locals
                .iter()
                .position(|candidate| frame.unit.names.get(candidate.index()) == Some(&name))
            && let Some(slot) = frame.slots_mut(&mut self.registers).get_mut(index)
        {
            let value = slot.take().unwrap_or_default();
            let reference = if value.is_reference() {
                value
            } else {
                Value::Reference(Rc::new(RefCell::new(value)))
            };
            *slot = Some(reference.clone());
            self.globals.globals.insert(name.clone(), reference);
        }
        let value = self.globals.globals.entry(name).or_default();
        if !value.is_reference() {
            *value = Value::Reference(Rc::new(RefCell::new(std::mem::take(value))));
        }
        let value = value.clone();
        self.set(local, value);
        Ok(())
    }
}
