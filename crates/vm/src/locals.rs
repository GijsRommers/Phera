//! PHP locals instruction handlers, separate from dispatch.

use phera_bytecode::{Instruction, Reg, TypeId};
use phera_extension_api::NativeError;
use phera_value::Value;

use crate::Vm;
use crate::calls::execution_loop::BatchExit;

impl Vm {
    /// Implements PHP Move semantics outside the dispatch loop.
    pub(crate) fn move_local(&mut self, destination: Reg, source: Reg) -> Result<(), NativeError> {
        let value = self.read_local(source);
        self.set(destination, value);
        Ok(())
    }

    /// Implements PHP Clear semantics outside the dispatch loop.
    pub(crate) fn clear_local(&mut self, register: Reg) -> Result<(), NativeError> {
        if let Some(slot) = self.current_slot_mut(register) {
            let previous = slot.take();
            if let Some(phera_value::Value::Reference(reference)) = previous {
                self.release_property_reference(reference);
            }
        }
        Ok(())
    }

    /// Implements PHP Assign semantics outside the dispatch loop.
    pub(crate) fn execute_assign(
        &mut self,
        local: Reg,
        source: Reg,
    ) -> Result<Option<BatchExit>, NativeError> {
        let value = self.get(source);
        if matches!(value, Value::Object(_)) && self.local_reference_needs_string(local, &value) {
            return Ok(Some(BatchExit::Instruction(Instruction::Assign {
                local,
                src: source,
            })));
        }
        self.restart_subject_iterators(local);
        self.assign_local(local, value)?;
        Ok(None)
    }

    /// The compiler uses the evaluated operand as the expression's result.
    /// A dead operand can be folded into a named local, which remains unchanged.
    pub(crate) fn set_cast_assignment_result(&mut self, source: Reg, value: phera_value::Value) {
        if self
            .current()
            .is_some_and(|frame| source.index() >= frame.function().locals.len())
        {
            self.set(source, value);
        }
    }

    /// Implements PHP CheckType semantics outside the dispatch loop.
    pub(crate) fn check_local_type(
        &mut self,
        local: Reg,
        declaration_type: TypeId,
    ) -> Result<(), NativeError> {
        let declaration_type = self
            .current()
            .and_then(|frame| frame.unit().types.get(declaration_type.index()))
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
