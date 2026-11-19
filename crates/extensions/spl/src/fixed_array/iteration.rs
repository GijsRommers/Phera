//! Fixed arrays iterate by index and expose each null slot.
//!
//! ```php
//! foreach (new SplFixedArray(2) as $index => $value) {}
//! ```

use phera_extension_api::{Context, NativeError, NativeIterator, NativeResult, Object, Value};
use std::rc::Rc;

use super::read;
use crate::{This, receiver};

pub(super) fn cursor(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Cursor { object, index: 0 }))
}

pub(super) fn get_iterator(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let cursor = cursor(context, receiver(this)?.clone())?;
    context.create_iterator(cursor)
}

struct Cursor {
    object: Rc<Object>,
    index: usize,
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.index = 0;
        Ok(())
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.index = self.index.saturating_add(1);
        Ok(())
    }

    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        read(&self.object, |fixed| self.index < fixed.slots.len())
    }

    fn current(&mut self, _: &mut dyn Context) -> NativeResult {
        read(&self.object, |fixed| {
            fixed.slots.get(self.index).cloned().unwrap_or(Value::Null)
        })
    }

    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        Ok(Value::Int(self.index as i64))
    }
}
