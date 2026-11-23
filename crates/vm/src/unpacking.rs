//! Array unpacking routes string keys by name and preserves writable element cells.

use crate::Vm;
use crate::calls::call_view::CallView;
use crate::calls::prepared_call::CallFlags;
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
    pub(crate) fn spread(&mut self, register: Reg) -> Result<bool, NativeError> {
        let source = self
            .current()
            .and_then(|frame| frame.slots(&self.registers).get(register.index()))
            .and_then(Option::as_ref)
            .cloned()
            .unwrap_or_default();
        let mut array = match &*source.deref() {
            Value::Array(array) => array.clone(),
            Value::Object(object) if object.class.is_traversable() => return Ok(false),
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
                key,
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
        Ok(true)
    }

    /// PHP binds integer keys positionally and rejects them after any string key.
    fn unpack_entry(&mut self, entry: &UnpackedEntry) -> Result<Value, NativeError> {
        let index = self.unpack_index(&entry.key)?;
        let view = CallView::top(&self.code, &self.call_stack)?;
        let parameters = view.parameters();
        let by_reference = parameters
            .receiving(index)
            .is_some_and(|parameter| parameters.by_reference(parameter));
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

    pub(crate) fn unpack_index(&mut self, key: &ArrayKey) -> Result<usize, NativeError> {
        match key {
            ArrayKey::Interned(name) => {
                self.call_stack.top_details_mut()?.named = Some(name.to_php_string())
            }
            ArrayKey::String(name) => self.call_stack.top_details_mut()?.named = Some(name.clone()),
            ArrayKey::Int(_) if self.call_stack.top_call()?.flags.contains(CallFlags::NAMED) => {
                return Err(NativeError::error(
                    "Cannot use positional argument after named argument during unpacking",
                ));
            }
            ArrayKey::Int(_) => {}
        }
        CallView::top(&self.code, &self.call_stack)?.argument_index()
    }
}
