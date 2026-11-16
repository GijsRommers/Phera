//! PHP locals instruction handlers, separate from dispatch.

use phera_bytecode::{Reg, TypeId};
use phera_extension_api::NativeError;

use crate::Vm;

impl Vm {
    /// Implements PHP Move semantics outside the dispatch loop.
    pub(crate) fn move_local(&mut self, dst: Reg, src: Reg) -> Result<(), NativeError> {
        let value = self.read_local(src);
        self.set(dst, value);
        Ok(())
    }
    /// Implements PHP Clear semantics outside the dispatch loop.
    pub(crate) fn clear_local(&mut self, reg: Reg) -> Result<(), NativeError> {
        if let Some(slot) = self
            .call_stack
            .frames
            .last_mut()
            .and_then(|frame| frame.slots_mut(&mut self.registers).get_mut(reg.index()))
        {
            *slot = None;
        }
        Ok(())
    }
    /// Implements PHP Assign semantics outside the dispatch loop.
    pub(crate) fn execute_assign(&mut self, local: Reg, src: Reg) -> Result<(), NativeError> {
        let value = self.get(src);
        if let Some(frame) = self.call_stack.frames.last_mut()
            && let Some(argument) = frame.arguments.get_mut(local.index())
        {
            *argument = value.clone();
        }
        self.assign_local(local, value)?;
        Ok(())
    }
    /// Implements PHP CheckType semantics outside the dispatch loop.
    pub(crate) fn check_local_type(&mut self, local: Reg, ty: TypeId) -> Result<(), NativeError> {
        let ty = self
            .current()
            .and_then(|frame| frame.unit.types.get(ty.index()))
            .cloned()
            .ok_or_else(|| NativeError::error("Phera internal error: type"))?;
        let value = crate::types::coerce(self.get(local), &ty, true, self)?;
        self.assign_local(local, value)?;
        Ok(())
    }
}
