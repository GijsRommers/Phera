//! Register windows and write-through PHP reference assignment.
use crate::Vm;
use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::Value;
impl Vm {
    pub(crate) fn read_local(&mut self, register: Reg) -> Value {
        if self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.index()))
            .is_some_and(Option::is_some)
        {
            return self.get(register);
        }
        if let Some(name) = self
            .current()
            .and_then(|frame| {
                frame
                    .function()
                    .locals
                    .get(register.index())
                    .and_then(|name| frame.unit.names.get(name.index()))
            })
            .cloned()
        {
            self.warn(format!("Undefined variable ${name}"));
        }
        Value::Null
    }

    pub(crate) fn assign_local(&mut self, register: Reg, value: Value) -> Result<(), NativeError> {
        if let Some(Value::Reference(reference)) = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.index()))
            .and_then(Option::as_ref)
        {
            *reference
                .try_borrow_mut()
                .map_err(|_| NativeError::error("Phera internal error: borrowed reference"))? =
                value;
        } else {
            self.set(register, value);
        }
        Ok(())
    }
}
