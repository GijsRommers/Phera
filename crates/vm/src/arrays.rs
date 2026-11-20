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
    Reference,
    Spread,
}

impl Vm {
    /// An undefined operand needs a continuation so its warning handler can run.
    pub(crate) fn array_write(
        &mut self,
        register: Reg,
        key: Option<Reg>,
        source: Reg,
        mode: ArrayWrite,
    ) -> Result<bool, NativeError> {
        let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
        let slots = diagnostics
            .vm
            .current()
            .ok_or_else(|| internal_error!("array frame"))?
            .slots(&diagnostics.vm.registers);
        let value = slots.get(source.index()).and_then(Option::as_ref);
        let key_value = key.and_then(|key| slots.get(key.index()).and_then(Option::as_ref));
        if key.is_some() && (value.is_none() || key_value.is_none()) {
            return Ok(false);
        }
        let value = array_element_value(value, mode);
        let key = key_value
            .map(|key| ArrayKey::from_value_diag(&key.deref(), &mut diagnostics.buffer))
            .transpose()?;
        drop(diagnostics);
        self.store_array_element(register, key, value, mode)?;
        Ok(true)
    }

    pub(crate) async fn array_write_with_warning(
        &mut self,
        register: Reg,
        key: Reg,
        source: Reg,
        mode: ArrayWrite,
    ) -> Result<(), NativeError> {
        let value_missing = self.warn_missing_operand(source).await?;
        let key = if self.warn_missing_operand(key).await? {
            // Undefined keys use the empty string without the null-key deprecation.
            ArrayKey::str("")
        } else {
            let mut diagnostics = crate::diagnostics::OperatorDiagnostics::new(self);
            ArrayKey::from_value_diag(&diagnostics.vm.operand(key), &mut diagnostics.buffer)?
        };
        let value = if value_missing {
            Value::Null
        } else {
            array_element_value(
                self.current()
                    .and_then(|frame| frame.slots(&self.registers).get(source.index()))
                    .and_then(Option::as_ref),
                mode,
            )
        };
        self.store_array_element(register, Some(key), value, mode)
    }

    /// Detach the array register before mutation so a temporary owner does not force COW.
    fn store_array_element(
        &mut self,
        register: Reg,
        key: Option<ArrayKey>,
        value: Value,
        mode: ArrayWrite,
    ) -> Result<(), NativeError> {
        if let Value::Reference(reference) = &value {
            self.state.track_cycle_reference(reference);
        }
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

fn array_element_value(value: Option<&Value>, mode: ArrayWrite) -> Value {
    match value {
        Some(value @ Value::Reference(_)) if mode == ArrayWrite::Element => value.deref().clone(),
        Some(value) => value.clone(),
        None => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    use phera_extension_api::Registry;
    use phera_value::{PhpArray, Value};

    use crate::{NativeTables, Vm};

    #[tokio::test]
    async fn cyclic_arrays_do_not_accumulate_across_requests() {
        let unit = Arc::new(
            phera_compiler::compile(
                "<?php $a ??= []; $a['self'] =& $a; echo 'ok';",
                "cyclic-array.php",
            )
            .unwrap(),
        );
        let native = NativeTables::new(Arc::new(Registry::new()));
        for _ in 0..2_000 {
            let cell = Rc::new(RefCell::new(Value::from(PhpArray::new())));
            let weak = Rc::downgrade(&cell);
            let mut vm = Vm::with_native_tables(&native);
            vm.set_global("a", Value::Reference(cell));
            assert_eq!(vm.run(unit.clone()).await, 0);
            assert_eq!(vm.output, b"ok");
            drop(vm);
            assert_eq!(weak.strong_count(), 0);
        }
    }
}
