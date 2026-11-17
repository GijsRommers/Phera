//! Array unpacking routes string keys by name and preserves writable element cells.

use crate::{Vm, calls::call_state::ArgumentOrder};
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
                return Err(NativeError::type_error(format!(
                    "Only arrays and Traversables can be unpacked, {} given",
                    source.type_name()
                )));
            }
        };
        let snapshot = array.clone();
        for (key, value) in snapshot.iter() {
            let entry = UnpackedEntry {
                key: key.clone(),
                value: value.clone(),
            };
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
        let call = self.call_stack.pending_mut()?;
        call.window.named = match &entry.key {
            ArrayKey::String(name) => Some(name.clone()),
            ArrayKey::Int(_) => {
                if call.window.order == ArgumentOrder::Named {
                    return Err(NativeError::error(
                        "Cannot use positional argument after named argument during unpacking",
                    ));
                }
                None
            }
        };
        let index = call.argument_index(&self.signatures)?;
        let by_reference = call
            .callee
            .parameter(index, &self.signatures)
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
        self.store_sent_value(index, value.clone())?;
        Ok(value)
    }
}
