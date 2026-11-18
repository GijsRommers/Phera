//! Global and static reference identity.

use std::{cell::RefCell, rc::Rc, sync::Arc};

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;

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
            .and_then(|frame| frame.unit().names.get(name.index()))
            .cloned()
            .ok_or_else(|| internal_error!("invalid name"))
    }

    /// Distinguish static cells by unit allocation, function and declared static slot.
    pub(crate) fn static_key(
        &self,
        variable: phera_bytecode::StaticId,
    ) -> Result<StaticKey, NativeError> {
        let frame = self.current().ok_or_else(|| internal_error!("static"))?;
        Ok(StaticKey {
            unit: Arc::as_ptr(frame.unit()) as usize,
            function: frame.function_id(),
            variable: variable.0,
        })
    }

    /// Share the main-scope cell with a function local rather than copying its value.
    pub(crate) fn bind_global(&mut self, local: Reg, name: String) -> Result<(), NativeError> {
        if let Some(frame) = self.frame_at(0)
            && let Some(index) = frame
                .function()
                .locals
                .iter()
                .position(|candidate| frame.unit().names.get(candidate.index()) == Some(&name))
            && let (base, count) = (frame.base(), frame.count())
            && let Some(slot) = self.registers.window_mut(base, count).get_mut(index)
        {
            let value = slot.take().unwrap_or_default();
            let reference = if value.is_reference() {
                value
            } else {
                Value::Reference(Rc::new(RefCell::new(value)))
            };
            *slot = Some(reference.clone());
            self.globals.insert(name.clone(), reference);
        }
        let value = self.globals.reference(name);
        self.set(local, value);
        Ok(())
    }
}
