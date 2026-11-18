//! WeakMap traversal skips released keys and may give foreach a value reference.
//!
//! ```php
//! foreach ($map as $key => &$value) $value++;
//! ```

use std::rc::Rc;

use phera_extension_api::{Context, NativeError, NativeIterator, NativeResult, Object, Value};

use super::{Map, references::current_reference};
use crate::{This, read_state, receiver};

pub(super) fn by_value(
    _: &mut dyn Context,
    map: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Cursor {
        map,
        index: 0,
        mode: Mode::Value,
    }))
}

pub(super) fn by_reference(
    _: &mut dyn Context,
    map: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Cursor {
        map,
        index: 0,
        mode: Mode::Reference,
    }))
}

pub(super) fn get_iterator(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let iterator = by_value(context, receiver(this)?.clone())?;
    context.create_iterator(iterator)
}

enum Mode {
    Value,
    Reference,
}

struct Cursor {
    map: Rc<Object>,
    index: usize,
    mode: Mode,
}

impl Cursor {
    fn entry(&self) -> Result<Option<(Rc<Object>, Value)>, NativeError> {
        read_state(&self.map, |state: &Map| {
            state
                .0
                .get_index(self.index)
                .and_then(|(_, (key, value))| Some((key.upgrade()?, value.clone())))
        })
    }
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
        let len = read_state(&self.map, |state: &Map| state.0.len())?;
        while self.index < len {
            if self.entry()?.is_some() {
                return Ok(true);
            }
            self.index += 1;
        }
        Ok(false)
    }
    fn current(&mut self, _: &mut dyn Context) -> NativeResult {
        let Some((key, value)) = self.entry()? else {
            return Ok(Value::Null);
        };
        match self.mode {
            Mode::Value => Ok(value.deref().clone()),
            Mode::Reference => current_reference(&self.map, &key),
        }
    }
    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        Ok(self
            .entry()?
            .map_or(Value::Null, |(object, _)| Value::Object(object)))
    }
}
