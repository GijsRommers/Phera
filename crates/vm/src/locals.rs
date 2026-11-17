//! PHP locals instruction handlers, separate from dispatch.

use phera_bytecode::{Reg, TypeId};
use phera_extension_api::NativeError;

use crate::Vm;

impl Vm {
    /// Implements PHP Move semantics outside the dispatch loop.
    pub(crate) fn move_local(&mut self, destination: Reg, source: Reg) -> Result<(), NativeError> {
        let value = self.read_local(source);
        self.set(destination, value);
        Ok(())
    }

    /// Implements PHP Clear semantics outside the dispatch loop.
    pub(crate) fn clear_local(&mut self, register: Reg) -> Result<(), NativeError> {
        if let Some(slot) = self.call_stack.current_mut().and_then(|frame| {
            frame
                .slots_mut(&mut self.registers)
                .get_mut(register.index())
        }) {
            let previous = slot.take();
            if let Some(phera_value::Value::Reference(reference)) = previous {
                self.release_property_reference(reference);
            }
        }
        Ok(())
    }

    /// Implements PHP Assign semantics outside the dispatch loop.
    pub(crate) fn execute_assign(&mut self, local: Reg, source: Reg) -> Result<(), NativeError> {
        let value = self.get(source);
        self.restart_subject_iterators(local);
        self.assign_local(local, value)?;
        Ok(())
    }

    /// Implements PHP CheckType semantics outside the dispatch loop.
    pub(crate) fn check_local_type(
        &mut self,
        local: Reg,
        declaration_type: TypeId,
    ) -> Result<(), NativeError> {
        let declaration_type = self
            .current()
            .and_then(|frame| frame.unit(&self.code).types.get(declaration_type.index()))
            .cloned()
            .ok_or_else(|| internal_error!("type"))?;
        let value = crate::types::coerce(
            self.get(local),
            &declaration_type,
            crate::types::TypeChecking::Strict,
            self,
        )?;
        self.restart_subject_iterators(local);
        self.assign_local(local, value)?;
        Ok(())
    }
}
