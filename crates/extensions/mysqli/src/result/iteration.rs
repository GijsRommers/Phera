//! `foreach ($result as $row)` traverses an associative cursor.

use phera_extension_api::{Context, NativeError, NativeIterator, NativeResult, Object, Value};
use std::rc::Rc;

use super::{Fetch, Handle, closed, handle};

pub fn iterator(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Cursor {
        handle: handle(&object)?,
        current: None,
        index: 0,
    }))
}

struct Cursor {
    handle: Handle,
    current: Option<Value>,
    index: i64,
}

impl Cursor {
    fn next(&mut self) -> Result<(), NativeError> {
        self.current = self
            .handle
            .borrow_mut()
            .as_mut()
            .ok_or_else(closed)?
            .array(Fetch::Assoc)?
            .map(Value::from);
        Ok(())
    }
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        if let Some(result) = self
            .handle
            .borrow_mut()
            .as_mut()
            .filter(|result| !result.unbuffered)
        {
            result.position = 0;
        }
        self.index = 0;
        self.next()
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.index = self.index.saturating_add(1);
        self.next()
    }

    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        Ok(self.current.is_some())
    }

    fn current(&mut self, _: &mut dyn Context) -> NativeResult {
        Ok(self.current.clone().unwrap_or(Value::Null))
    }

    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        Ok(Value::Int(self.index))
    }
}
