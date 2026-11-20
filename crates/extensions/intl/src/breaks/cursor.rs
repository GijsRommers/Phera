//! The PHP InternalIterator traverses the same cursor as IntlBreakIterator methods.

use std::rc::Rc;

use phera_extension_api::{Context, NativeError, NativeIterator, NativeResult, Object, Value};

use super::{missing, update};

pub fn wrapping(context: &mut dyn Context, object: Option<&Rc<Object>>) -> NativeResult {
    let cursor = make(context, object.ok_or_else(missing)?.clone())?;
    context.create_iterator(cursor)
}

pub fn make(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Cursor {
        object,
        index: 0,
        exhausted: false,
    }))
}

struct Cursor {
    object: Rc<Object>,
    index: i64,
    exhausted: bool,
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.index = 0;
        self.exhausted = false;
        update(&self.object, |state| {
            state.position(0);
        })
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.index = self.index.saturating_add(1);
        self.exhausted = update(&self.object, |state| state.step(1))? < 0;
        Ok(())
    }

    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        Ok(!self.exhausted)
    }

    fn current(&mut self, _: &mut dyn Context) -> NativeResult {
        if self.exhausted {
            return Ok(Value::Null);
        }
        update(&self.object, |state| Value::Int(state.current() as i64))
    }

    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        Ok(if self.exhausted {
            Value::Null
        } else {
            Value::Int(self.index)
        })
    }
}
