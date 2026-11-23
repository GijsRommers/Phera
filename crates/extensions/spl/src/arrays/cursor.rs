//! ArrayIterator keeps its cursor on the object, even when two loops use it.
//!
//! ```php
//! $i = new ArrayIterator(['a', 'b']);
//! $i->next();
//! echo $i->current(); // b
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeIterator, NativeResult, Object, PhpArray, Value, args,
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
    };
    visible_at(array, index, properties)
}

fn inspect<T>(
    object: &Object,
    f: impl FnOnce(&PhpArray, Option<usize>) -> T,
) -> Result<T, NativeError> {
    let (f, position) = match state::read(object, |state| {
        let position = state.position;
        match &state.storage {
            state::Storage::Array(array) => Ok(f(array, index(array, position, false))),
            _ => Err((f, position)),
        }
    })? {
        Ok(value) => return Ok(value),
        Err(remote) => remote,
    };
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
        let properties = target.is_properties();
        let next = index(array, position, properties).and_then(|slot| array.next_position(slot));
        visible_at(array, next, properties).unwrap_or_else(|| array.end_position())
    })?;
    move_cursor(object, Position::Slot(next))
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
        let (found, end) = target.read(|array| {
            let position = if target.is_properties() {
                let mut position = visible_at(array, array.first_position(), true);
                for _ in 0..offset {
                    let Some(slot) = position else { break };
                    position = visible_at(array, array.next_position(slot), true);
                }
                position
            } else {
                array.position_at(offset)
            };
            (position, array.end_position())
        })?;
        move_cursor(object, Position::Slot(found.unwrap_or(end)))?;
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
    Ok(Box::new(Cursor {
        object,
        mode: CursorMode::Value,
        started: false,
    }))
}

pub(super) fn create_by_reference(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Cursor {
        object,
        mode: CursorMode::Reference,
        started: false,
    }))
}

enum CursorMode {
    Value,
    Reference,
}

struct Cursor {
    object: Rc<Object>,
    mode: CursorMode,
    started: bool,
}

fn current_reference(context: &mut dyn Context, object: &Object) -> NativeResult {
    let Some(position) = inspect(object, |_, index| index)? else {
        return Ok(Value::Null);
    };
    Target::of(object)?.update(|array| {
        array
            .entry_at_mut(position)
            .map(|(_, slot)| super::dimensions::element_reference(context, slot))
            .ok_or_else(|| NativeError::error("SPL array entry vanished"))
    })
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.started = false;
        move_cursor(&self.object, Position::Initial)
    }

    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        advance(&self.object)
    }

    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        inspect(&self.object, |_, index| index.is_some())
    }

    fn current(&mut self, context: &mut dyn Context) -> NativeResult {
        self.started = true;
        match self.mode {
            CursorMode::Value => current_value(&self.object),
            CursorMode::Reference => current_reference(context, &self.object),
        }
    }

    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        current_key(&self.object)
    }

    fn next_entry(&mut self, with_key: bool) -> Option<(Value, Option<Value>)> {
        if matches!(self.mode, CursorMode::Reference) {
            return None;
        }
        let result = state::update(&self.object, |state| {
            let position = state.position;
            let entry = match &state.storage {
                state::Storage::Array(array) => self.entry(array, position, with_key),
                state::Storage::Alias(alias) => state::read(alias, |owner| {
                    let state::Storage::Array(array) = &owner.storage else {
                        return None;
                    };
                    self.entry(array, position, with_key)
                })?,
                _ => None,
            };
            Ok(entry.map(|(position, row)| {
                if self.started {
                    state.position = Position::Slot(position);
                }
                row
            }))
        })
        .ok()??;
        self.started = true;
        Some(result)
    }
}

impl Cursor {
    fn entry(
        &self,
        array: &PhpArray,
        position: Position,
        with_key: bool,
    ) -> Option<(usize, (Value, Option<Value>))> {
        let mut slot = index(array, position, false)?;
        if self.started {
            slot = array.next_position(slot)?;
        }
        let (key, value) = array.entry_at(slot)?;
        Some((slot, (value.deref().clone(), with_key.then(|| key.into()))))
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

pub(super) fn children(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let (value, valid) = inspect(object, |array, index| {
            (
                index.map_or(Value::Null, |index| state::value_at(array, index)),
                index.is_some(),
            )
        })?;
        if !valid {
            return Ok(Value::Null);
        }
        let flags = state::read(object, |state| state.flags)?;
        if let Value::Object(child) = &value {
            if flags.child_arrays_only() {
                return Ok(Value::Null);
            }
            if child.class.is_instance_of(object.class.name()) {
                return Ok(value);
            }
        }
        if let Value::Array(array) = &value
            && object.class.name() == super::RECURSIVE
        {
            let child = crate::make(
                context,
                super::RECURSIVE,
                state::State {
                    flags,
                    storage: state::Storage::Array(array.clone()),
                    ..state::State::default()
                },
            )?;
            return Ok(Value::Object(child));
        }
        let output = context.create_object(object.class.name(), PhpArray::new())?;
        let Value::Object(child) = &output else {
            return Err(NativeError::error(
                "Cannot create RecursiveArrayIterator child",
            ));
        };
        context
            .call_method(
                child.clone(),
                "__construct",
                vec![value, Value::Int(flags.raw())],
            )
            .await?;
        Ok(output)
    })
}
