//! Array unpacking routes string keys by name and preserves writable element cells.

use crate::{Vm, call_state::ArgumentOrder};
use phera_bytecode::Reg;
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, Value};
use std::{cell::RefCell, rc::Rc};

/// One entry can be promoted after separating the source's copy-on-write array.
struct UnpackedEntry {
    key: ArrayKey,
    value: Value,
}

impl Vm {
    /// Writable source arrays receive the same cells bound to by-reference parameters.
    pub(crate) fn spread(&mut self, register: Reg) -> Result<(), NativeError> {
        let source = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.index()))
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default();
        let mut array = match &*source.deref() {
            Value::Array(array) => array.clone(),
            _ => {
                return Err(NativeError::type_error(
                    "Only arrays and Traversables can be unpacked",
                ));
            }
        };
        let entries: Vec<_> = array
            .iter()
            .map(|(key, value)| UnpackedEntry {
                key: key.clone(),
                value: value.clone(),
            })
            .collect();
        for entry in entries {
            let value = self.unpack_entry(&entry)?;
            if value.is_reference() {
                Rc::make_mut(&mut array).insert(entry.key, value);
            }
        }
        if let Value::Reference(reference) = source {
            *reference
                .try_borrow_mut()
                .map_err(|_| internal_error!("unpack source"))? = Value::Array(array);
        }
        Ok(())
    }

    /// PHP binds integer keys positionally and rejects them after any string key.
    fn unpack_entry(&mut self, entry: &UnpackedEntry) -> Result<Value, NativeError> {
        let call = self
            .call_stack
            .current_mut()
            .and_then(|frame| frame.calls.last_mut())
            .ok_or_else(|| internal_error!("unpack without call"))?;
        call.named = match &entry.key {
            ArrayKey::String(name) => Some(name.to_string_lossy()),
            ArrayKey::Int(_) => {
                if call.order() == ArgumentOrder::Named {
                    return Err(NativeError::error(
                        "Cannot use positional argument after named argument during unpacking",
                    ));
                }
                None
            }
        };
        let index = call.argument_index()?;
        let by_reference = call
            .callee
            .parameters
            .get(index)
            .or_else(|| {
                call.callee
                    .parameters
                    .last()
                    .filter(|parameter| parameter.variadic)
            })
            .is_some_and(|parameter| parameter.by_reference);
        let value = if by_reference {
            if entry.value.is_reference() {
                entry.value.clone()
            } else {
                Value::Reference(Rc::new(RefCell::new(entry.value.deref().clone())))
            }
        } else {
            entry.value.deref().clone()
        };
        call.store_argument(index, call.named.clone(), value.clone())?;
        Ok(value)
    }
}
