//! The storage iterator exposes objects as values and positions as integer keys.
//!
//! ```php
//! foreach ($storage as $index => $object) echo $storage->getInfo();
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeIterator, NativeResult, Object, Value, args,
};

use super::{Storage, attach, detach, hash, hash_scope::require_mutation_allowed, read, update};
use crate::{This, receiver};

fn rewind_cursor(object: &Object) -> Result<(), NativeError> {
    update(object, |state| {
        state.index = 0;
        state.slot = 0;
    })
}

fn advance_cursor(object: &Object) -> Result<(), NativeError> {
    update(object, |state| {
        state.slot = state.slot.saturating_add(1);
        state.index = state.index.saturating_add(1);
    })
}

fn current_object(object: &Object) -> NativeResult {
    read(object, |state| {
        state
            .entries
            .get_index(state.slot)
            .map(|(_, entry)| entry.object.clone())
    })?
    .ok_or_else(|| NativeError::throw("RuntimeException", "Called current() on invalid iterator"))
}

fn is_valid(object: &Object) -> Result<bool, NativeError> {
    read(object, |state| state.slot < state.entries.len())
}

fn current_key(object: &Object) -> NativeResult {
    read(object, |state| Value::Int(state.index))
}

pub(super) fn create(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Cursor {
        object,
        started: false,
    }))
}

struct Cursor {
    object: Rc<Object>,
    started: bool,
}

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        self.started = false;
        rewind_cursor(&self.object)
    }
    fn advance(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        advance_cursor(&self.object)
    }
    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        is_valid(&self.object)
    }
    fn current(&mut self, _: &mut dyn Context) -> NativeResult {
        self.started = true;
        current_object(&self.object)
    }
    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        current_key(&self.object)
    }

    fn next_entry(&mut self, with_key: bool) -> Option<(Value, Option<Value>)> {
        let result = update(&self.object, |storage| {
            let slot = storage.slot.saturating_add(usize::from(self.started));
            let (_, entry) = storage.entries.get_index(slot)?;
            let value = entry.object.clone();
            storage.slot = slot;
            storage.index = storage.index.saturating_add(i64::from(self.started));
            let key = with_key.then_some(Value::Int(storage.index));
            Some((value, key))
        })
        .ok()??;
        self.started = true;
        Some(result)
    }
}

pub(super) fn rewind(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    rewind_cursor(receiver(this)?)?;
    Ok(Value::Null)
}

pub(super) fn next(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    advance_cursor(receiver(this)?)?;
    Ok(Value::Null)
}

pub(super) fn valid(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    is_valid(receiver(this)?).map(Value::Bool)
}

pub(super) fn current(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    current_object(receiver(this)?)
}

pub(super) fn key(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    current_key(receiver(this)?)
}

pub(super) fn seek(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let offset = args::int(args, 0).unwrap_or_default();
    update(receiver(this)?, |state| {
        let slot = usize::try_from(offset)
            .ok()
            .filter(|slot| *slot < state.entries.len());
        let Some(slot) = slot else {
            return Err(NativeError::throw(
                "OutOfBoundsException",
                format!("Seek position {offset} is out of range"),
            ));
        };
        state.slot = slot;
        state.index = offset;
        Ok(Value::Null)
    })?
}

pub(super) fn get_info(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| {
        state
            .entries
            .get_index(state.slot)
            .map_or(Value::Null, |(_, entry)| entry.info.deref().clone())
    })
}

pub(super) fn set_info<const CHECK: bool>(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    require_mutation_allowed::<CHECK>(context)?;
    update(receiver(this)?, |state| {
        if let Some((_, entry)) = state.entries.get_index_mut(state.slot) {
            entry.info = args::value(args, 0).deref().clone();
        }
        Value::Null
    })
}

pub(super) fn native_count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    read(object, |state| state.entries.len() as i64)
}

pub(super) fn count(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    native_count(context, receiver(this)?).map(Value::Int)
}

pub(super) fn get_hash(_: &mut dyn Context, _: This<'_>, args: &[Value]) -> NativeResult {
    let object =
        args::object(args, 0).ok_or_else(|| NativeError::error("Missing object to hash"))?;
    Ok(Value::from(object.handle_hash()))
}

pub(super) fn clone_storage(original: &Rc<Object>, copy: &Object) -> Result<(), NativeError> {
    let entries = read(original, |state| state.entries.clone())?;
    crate::store(
        copy,
        Storage {
            entries,
            ..Storage::default()
        },
    )
}

#[derive(Clone, Copy)]
enum Bulk {
    Add,
    Remove,
    Keep,
}

fn bulk<'a, const CHECK: bool>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
    operation: Bulk,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let other = receiver(args::object(&args, 0))?;
        let source = if matches!(operation, Bulk::Keep) {
            object
        } else {
            other
        };
        let entries = read(source, |state| {
            state.entries.values().cloned().collect::<Vec<_>>()
        })?;
        for entry in entries {
            let Value::Object(item) = &entry.object else {
                continue;
            };
            match operation {
                Bulk::Add => {
                    require_mutation_allowed::<CHECK>(context)?;
                    let key = hash(context, object, item).await?;
                    attach(object, key, item.clone(), entry.info)?;
                }
                Bulk::Remove => {
                    require_mutation_allowed::<CHECK>(context)?;
                    let key = hash(context, object, item).await?;
                    detach(object, &key, false)?;
                }
                Bulk::Keep => {
                    let other_key = hash(context, other, item).await?;
                    if !read(other, |state| state.entries.contains_key(&other_key))? {
                        require_mutation_allowed::<CHECK>(context)?;
                        let key = hash(context, object, item).await?;
                        detach(object, &key, false)?;
                    }
                }
            }
        }
        if !matches!(operation, Bulk::Add) {
            rewind_cursor(object)?;
        }
        native_count(context, object).map(Value::Int)
    })
}

pub(super) fn add_all<const CHECK: bool>(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    bulk::<CHECK>(context, this, args, Bulk::Add)
}

pub(super) fn remove_all<const CHECK: bool>(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    bulk::<CHECK>(context, this, args, Bulk::Remove)
}

pub(super) fn remove_all_except<const CHECK: bool>(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    args: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    bulk::<CHECK>(context, this, args, Bulk::Keep)
}
