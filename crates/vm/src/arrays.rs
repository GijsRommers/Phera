//! In-place array construction without keeping an extra COW owner.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Value};

use crate::Vm;

/// Literal construction either inserts one element or unpacks an entire array.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrayWrite {
    Element,
    Spread,
}

impl Vm {
    /// Detach the array register before mutation so a temporary owner does not force COW.
    pub(crate) fn array_write(
        &mut self,
        register: Reg,
        key: Option<Reg>,
        source: Reg,
        mode: ArrayWrite,
    ) -> Result<(), NativeError> {
        let value = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(source.index()))
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default();
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let key = key
            .map(|key| {
                ArrayKey::from_value_diag(&diagnostics.vm.operand(key), &mut diagnostics.buffer)
            })
            .transpose()?;
        drop(diagnostics);
        let mut target = self
            .current_slot_mut(register)
            .and_then(Option::take)
            .unwrap_or_default();
        let Value::Array(array) = &mut target else {
            return Err(internal_error!("array"));
        };
        let array = Rc::make_mut(array);
        if mode == ArrayWrite::Spread {
            let Value::Array(source) = value else {
                return Err(NativeError::error(
                    "Only arrays and Traversables can be unpacked",
                ));
            };
            for (key, value) in source.iter() {
                if matches!(key, ArrayKey::Int(_)) {
                    array.push(value.clone())?;
                } else {
                    array.insert(key, value.clone());
                }
            }
        } else if let Some(key) = key {
            array.insert(key, value);
        } else {
            array.push(value)?;
        }
        self.set(register, target);
        Ok(())
    }
}
