//! Foreach has its own linked list cursor and snapshots the selected iterator mode.
//!
//! ```php
//! foreach ($queue as $value) { echo $value; }
//! var_dump($queue->current()); // unchanged by foreach
//! ```

use std::rc::Rc;

use phera_extension_api::{Context, NativeError, NativeIterator, NativeResult, Object, Value};

use super::{Consumption, List, Mode, Order, Position, read, update};
use crate::{This, receiver};

impl Position {
    fn rewind(list: &List, order: Order) -> Self {
        let len = list.entries.len();
        let (index, key) = match order {
            Order::Fifo => (Some(0), 0),
            Order::Lifo => (len.checked_sub(1), len as i64 - 1),
        };
        let entry = index.and_then(|slot| Some((slot, list.entries.get(slot)?.identity)));
        Self { entry, key }
    }

    fn locate(self, list: &List) -> Option<usize> {
        let (hint, identity) = self.entry?;
        if list
            .entries
            .get(hint)
            .is_some_and(|entry| entry.identity == identity)
        {
            return Some(hint);
        }
        list.entries
            .iter()
            .position(|entry| entry.identity == identity)
    }

    fn advance(&mut self, list: &mut List, mode: Mode) {
        let Some(position) = self.locate(list) else {
            self.entry = None;
            return;
        };
        let next = match mode.order {
            Order::Fifo => position.checked_add(1),
            Order::Lifo => position.checked_sub(1),
        };
        let entry = next.and_then(|slot| Some((slot, list.entries.get(slot)?.identity)));
        self.entry = match (mode.order, mode.consumption) {
            (Order::Fifo, Consumption::Delete) => {
                list.entries.pop_front();
                entry.map(|(slot, id)| (slot.saturating_sub(1), id))
            }
            (Order::Fifo, Consumption::Keep) => {
                self.key = self.key.saturating_add(1);
                entry
            }
            (Order::Lifo, consumption) => {
                self.key = self.key.saturating_sub(1);
                if matches!(consumption, Consumption::Delete) {
                    list.entries.pop_back();
                }
                entry
            }
        };
    }

    fn value(self, list: &List) -> Value {
        self.locate(list)
            .and_then(|slot| list.entries.get(slot))
            .map_or(Value::Null, |entry| entry.value.deref().clone())
    }
}

pub(super) fn create(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    let mode = read(&object, |list| list.mode(&object))?;
    Ok(Box::new(Cursor {
        object,
        mode,
        position: Position::default(),
    }))
}

struct Cursor {
    object: Rc<Object>,
    mode: Mode,
    position: Position,
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.position = read(&self.object, |list| Position::rewind(list, self.mode.order))?;
        Ok(())
    }
    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        update(&self.object, |list| self.position.advance(list, self.mode))
    }
    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        read(&self.object, |list| self.position.locate(list).is_some())
    }
    fn current(&mut self, _: &mut dyn Context) -> NativeResult {
        read(&self.object, |list| self.position.value(list))
    }
    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        Ok(Value::Int(self.position.key))
    }
}

pub(super) fn rewind(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    update(object, |list| {
        list.cursor = Position::rewind(list, list.mode(object).order);
        Value::Null
    })
}

fn advance(this: This<'_>, reverse: bool) -> NativeResult {
    let object = receiver(this)?;
    update(object, |list| {
        let mut position = list.cursor;
        let mut mode = list.mode(object);
        if reverse {
            mode.order = mode.order.reversed();
        }
        position.advance(list, mode);
        list.cursor = position;
        Value::Null
    })
}

pub(super) fn next(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    advance(this, false)
}

pub(super) fn prev(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    advance(this, true)
}

pub(super) fn current(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |list| list.cursor.value(list))
}

pub(super) fn key(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |list| Value::Int(list.cursor.key))
}

pub(super) fn valid(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |list| {
        Value::Bool(list.cursor.locate(list).is_some())
    })
}
