//! Heap foreach removes each element when advancing.
//!
//! ```php
//! foreach ($heap as $remaining => $top) { echo $top; }
//! var_dump($heap->isEmpty()); // true
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeIterator, NativeResult, Object, Value, args,
};

use super::{
    Entry, Extract, PRIORITY_QUEUE, Status, read, runtime,
    sift::{self, Change},
    update,
};
use crate::{This, receiver};

pub(super) fn insert(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let mut arguments = arguments.into_iter();
        let data = arguments.next().unwrap_or(Value::Null);
        let priority = arguments.next();
        sift::apply(context, object, Change::Insert(Entry { data, priority })).await?;
        Ok(Value::Bool(true))
    })
}

pub(super) fn extract(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let flags = read(object, |heap| heap.projection(object))?;
        let entry = sift::apply(context, object, Change::Remove)
            .await?
            .ok_or_else(|| runtime("Can't extract from an empty heap"))?;
        Ok(flags.project(&entry))
    })
}

enum Peek {
    Top,
    Current,
}

fn peek(object: &Object, mode: Peek) -> NativeResult {
    read(object, |heap| {
        heap.validate(false)?;
        let Some(entry) = heap.items.first() else {
            return match mode {
                Peek::Top => Err(runtime("Can't peek at an empty heap")),
                Peek::Current => Ok(Value::Null),
            };
        };
        Ok(heap.projection(object).project(entry))
    })?
}

pub(super) fn top(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    peek(receiver(this)?, Peek::Top)
}

pub(super) fn current(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    peek(receiver(this)?, Peek::Current)
}

pub(super) fn rewind(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Null)
}

pub(super) fn next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        sift::apply(context, receiver(this.as_ref())?, Change::Remove).await?;
        Ok(Value::Null)
    })
}

pub(super) fn native_count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    read(object, |heap| heap.items.len() as i64)
}

pub(super) fn count(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    native_count(context, receiver(this)?).map(Value::Int)
}

pub(super) fn valid(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |heap| Value::Bool(!heap.items.is_empty()))
}

pub(super) fn is_empty(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |heap| Value::Bool(heap.items.is_empty()))
}

fn current_key(object: &Object) -> NativeResult {
    read(object, |heap| Value::Int(heap.items.len() as i64 - 1))
}

pub(super) fn key(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    current_key(receiver(this)?)
}

pub(super) fn recover(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    update(receiver(this)?, |heap| {
        if matches!(heap.status, Status::Corrupted) {
            heap.status = Status::Ready;
        }
        Value::Bool(true)
    })
}

pub(super) fn corrupted(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |heap| {
        Value::Bool(matches!(heap.status, Status::Corrupted))
    })
}

pub(super) fn set_flags(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let flags = args::int(arguments, 0).unwrap_or_default();
    let flags = Extract::from_flags(flags)
        .ok_or_else(|| runtime("Must specify at least one extract flag"))?;
    update(receiver(this)?, |heap| {
        heap.extract = flags;
        Value::Int(flags.flags())
    })
}

pub(super) fn get_flags(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |heap| Value::Int(heap.extract.flags()))
}

pub(super) fn compare_max(context: &mut dyn Context, _: This<'_>, args: &[Value]) -> NativeResult {
    sift::compare_values(context, args::value(args, 0), args::value(args, 1), false)
}

pub(super) fn compare_min(context: &mut dyn Context, _: This<'_>, args: &[Value]) -> NativeResult {
    sift::compare_values(context, args::value(args, 0), args::value(args, 1), true)
}

pub(super) fn cursor(
    _: &mut dyn Context,
    object: Rc<Object>,
) -> Result<Box<dyn NativeIterator>, NativeError> {
    Ok(Box::new(Cursor(object)))
}

struct Cursor(Rc<Object>);

impl NativeIterator for Cursor {
    fn rewind(&mut self, _: &mut dyn Context) -> Result<(), NativeError> {
        Ok(())
    }
    fn advance(&mut self, context: &mut dyn Context) -> Result<(), NativeError> {
        sift::apply_native(context, &self.0, Change::Remove).map(drop)
    }
    fn valid(&mut self, _: &mut dyn Context) -> Result<bool, NativeError> {
        read(&self.0, |heap| !heap.items.is_empty())
    }
    fn current(&mut self, _: &mut dyn Context) -> NativeResult {
        peek(&self.0, Peek::Current)
    }
    fn key(&mut self, _: &mut dyn Context) -> NativeResult {
        current_key(&self.0)
    }
}

pub(super) fn base(object: &Object) -> &'static str {
    if object.class.is_instance_of(PRIORITY_QUEUE) {
        PRIORITY_QUEUE
    } else {
        super::HEAP
    }
}
