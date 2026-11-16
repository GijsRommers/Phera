//! Named variadic arguments retain their source-order string keys after parameter coercion.

use crate::Vm;
use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, PhpArray, Value};
use std::rc::Rc;

impl Vm {
    /// Parameter binding already checked each element; restore names without coercing twice.
    pub(crate) fn name_variadic_arguments(
        &mut self,
        named_start: usize,
        names: Vec<String>,
    ) -> Result<(), NativeError> {
        if names.is_empty() {
            return Ok(());
        }
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("named variadic without frame"))?;
        let Some(index) = frame
            .function()
            .parameters
            .iter()
            .position(|parameter| parameter.variadic)
        else {
            return Ok(());
        };
        let register = Reg(index as u16);
        let Value::Array(array) = self.get(register) else {
            return Err(internal_error!("variadic without array"));
        };
        let mut renamed = PhpArray::new();
        let positional_count = named_start.saturating_sub(index);
        for (offset, value) in array.values().enumerate() {
            if let Some(name) = offset
                .checked_sub(positional_count)
                .and_then(|offset| names.get(offset))
            {
                renamed.insert(ArrayKey::str(name.as_str()), value.clone());
            } else {
                renamed.push(value.clone())?;
            }
        }
        self.set(register, Value::Array(Rc::new(renamed)));
        Ok(())
    }
}
