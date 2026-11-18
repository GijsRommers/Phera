//! ArrayIterator keeps its cursor on the object, even when two loops use it.
//!
//! ```php
//! $i = new ArrayIterator(['a', 'b']);
//! $i->next();
//! echo $i->current(); // b
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, NativeError, NativeIterator, NativeResult, Object, PhpArray, Value, args,
};

use super::state::{self, Position, Target};
use crate::{This, receiver};

fn visible_at(array: &PhpArray, mut slot: Option<usize>, properties: bool) -> Option<usize> {
    if !properties {
        return slot;
    }
    while let Some(index) = slot {
        if array
            .entry_at(index)
            .is_some_and(|(key, _)| state::public(&key))
        {
            return slot;
        }
        slot = array.next_position(index);
    }
    None
}

fn index(array: &PhpArray, position: Position, properties: bool) -> Option<usize> {
    let index = match position {
        Position::Initial => array.first_position(),
        Position::Slot(index) => array
            .entry_at(index)
            .map(|_| index)
            .or_else(|| array.next_position(index)),
        Position::End => None,
    };
    visible_at(array, index, properties)
}

fn inspect<T>(
    object: &Object,
    f: impl FnOnce(&PhpArray, Option<usize>) -> T,
) -> Result<T, NativeError> {
    let position = state::read(object, |state| state.position)?;
    let target = Target::of(object)?;
    target.read(|array| f(array, index(array, position, target.is_properties())))
}

fn move_cursor(object: &Object, next: Position) -> Result<(), NativeError> {
    state::update(object, |state| {
        state.position = next;
        Ok(())
    })
}

fn advance(object: &Object) -> Result<(), NativeError> {
    let position = state::read(object, |state| state.position)?;
    let target = Target::of(object)?;
    let next = target.read(|array| {
        let start = match position {
            Position::Initial => array
                .first_position()
                .and_then(|slot| array.next_position(slot)),
            Position::Slot(slot) => array.next_position(slot),
            Position::End => None,
        };
        visible_at(array, start, target.is_properties())
    })?;
    move_cursor(object, next.map_or(Position::End, Position::Slot))
}

fn current_value(object: &Object) -> NativeResult {
    inspect(object, |array, index| {
        index.map_or(Value::Null, |index| state::value_at(array, index))
    })
}

fn current_key(object: &Object) -> NativeResult {
    inspect(object, |array, index| {
        index
            .and_then(|index| array.entry_at(index))
            .map_or(Value::Null, |(key, _)| key.into())
    })
}

pub(super) fn rewind(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    move_cursor(receiver(this)?, Position::Initial)?;
    Ok(Value::Null)
}

pub(super) fn next(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    advance(receiver(this)?)?;
    Ok(Value::Null)
}

pub(super) fn current(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    current_value(receiver(this)?)
}

pub(super) fn key(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    current_key(receiver(this)?)
}

pub(super) fn valid(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(inspect(receiver(this)?, |_, index| {
        index.is_some()
    })?))
}

pub(super) fn seek(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let offset = args::int(args, 0).unwrap_or_default();
    if let Ok(offset) = usize::try_from(offset) {
        let target = Target::of(object)?;
        let found = target.read(|array| {
            if target.is_properties() {
                return array
                    .keys()
                    .filter(state::public)
                    .nth(offset)
                    .and_then(|key| {
                        let mut position = array.first_position();
                        while let Some(slot) = position {
                            if array.entry_at(slot).is_some_and(|(other, _)| other == key) {
                                return position;
                            }
                            position = array.next_position(slot);
                        }
                        None
                    });
            }
            let mut position = array.first_position();
            for _ in 0..offset {
                position = array.next_position(position?);
            }
            position
        })?;
        move_cursor(object, found.map_or(Position::End, Position::Slot))?;
        if found.is_some() {
            return Ok(Value::Null);
        }
    }
    Err(NativeError::throw(
        "OutOfBoundsException",
        format!("Seek position {offset} is out of range"),
    ))
}

pub(super) fn create(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Cursor(object)))
}

struct Cursor(Rc<Object>);

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        move_cursor(&self.0, Position::Initial)
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        advance(&self.0)
    }

    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        inspect(&self.0, |_, index| index.is_some())
    }

    fn current(&mut self, _: &mut dyn Context) -> NativeResult {
        current_value(&self.0)
    }

    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        current_key(&self.0)
    }
}

pub(super) fn has_children(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let value = current_value(object)?;
    let only_arrays = state::read(object, |state| state.flags.child_arrays_only())?;
    Ok(Value::Bool(
        matches!(value, Value::Array(_)) || (!only_arrays && matches!(value, Value::Object(_))),
    ))
}

pub(super) fn children(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let value = current_value(object)?;
    let flags = state::read(object, |state| state.flags)?;
    let storage = match value {
        Value::Array(array) => state::Storage::Array(array),
        Value::Object(value) if !flags.child_arrays_only() => {
            if value.class.is_instance_of(super::RECURSIVE) {
                return Ok(Value::Object(value));
            }
            state::Storage::Properties(value)
        }
        _ => return Ok(Value::Null),
    };
    let child = crate::make(
        context,
        object.class.name(),
        state::State {
            flags,
            storage,
            ..state::State::default()
        },
    )?;
    Ok(Value::Object(child))
}
