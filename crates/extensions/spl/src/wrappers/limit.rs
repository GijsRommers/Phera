//! LimitIterator tracks an absolute position and checks its permitted range.
//!
//! ```php
//! $page = new LimitIterator($items, 10, 5);
//! $page->seek(12);
//! ```

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, Value, args, traversal::IteratorStep,
    value::to_bool,
};
use std::rc::Rc;

use super::{Wrapper, clear, fetch, inner, new_wrapper, next, read, rewind, step, update};
use crate::{This, receiver};

#[derive(Clone, Copy)]
pub(super) struct Limit {
    start: i64,
    length: Option<i64>,
}

impl Limit {
    fn beyond(self, position: i64) -> bool {
        self.length
            .is_some_and(|length| position.saturating_sub(self.start) >= length)
    }

    fn check(self, position: i64) -> Result<(), NativeError> {
        if position < self.start {
            return Err(NativeError::throw(
                "OutOfBoundsException",
                format!(
                    "Cannot seek to {position} which is below the offset {}",
                    self.start,
                ),
            ));
        }
        if self.beyond(position) {
            return Err(NativeError::throw(
                "OutOfBoundsException",
                format!(
                    "Cannot seek to {position} which is behind offset {} plus count {}",
                    self.start,
                    self.length.unwrap_or_default(),
                ),
            ));
        }
        Ok(())
    }
}

fn limit(object: &Object) -> Result<Limit, NativeError> {
    read(object, |state| state.limit)?.ok_or_else(super::uninitialized)
}

pub(super) fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let start = args::int(&arguments, 1).unwrap_or_default();
        let length = args::int(&arguments, 2).unwrap_or(-1);
        if start < 0 {
            return Err(NativeError::value_error(
                "LimitIterator::__construct(): Argument #2 ($offset) must be greater than or equal to 0",
            ));
        }
        if length < -1 {
            return Err(NativeError::value_error(
                "LimitIterator::__construct(): Argument #3 ($limit) must be greater than or equal to -1",
            ));
        }
        let inner = args::object(&arguments, 0)
            .ok_or_else(super::uninitialized)?
            .clone();
        new_wrapper(
            context,
            receiver(this.as_ref())?,
            inner,
            Wrapper {
                limit: Some(Limit {
                    start,
                    length: (length != -1).then_some(length),
                }),
                ..Wrapper::default()
            },
        )
        .await
    })
}

async fn seek_to(
    context: &mut dyn Context,
    object: &Rc<Object>,
    target: i64,
) -> Result<i64, NativeError> {
    clear(object)?;
    let limit = limit(object)?;
    limit.check(target)?;
    let mut position = read(object, |state| state.position)?;
    let inner = inner(object)?;
    if position != target && inner.class.is_instance_of("SeekableIterator") {
        context
            .call_method(inner, "seek", vec![Value::Int(target)])
            .await?;
        position = target;
        update(object, |state| state.position = position)?;
    } else {
        if target < position {
            rewind(context, object).await?;
            position = 0;
        }
        while position < target && to_bool(&step(context, object, IteratorStep::Valid).await?) {
            next(context, object).await?;
            position = position.saturating_add(1);
        }
    }
    fetch(context, object).await?;
    Ok(position)
}

pub(super) fn seek(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let target = args::int(&arguments, 0).unwrap_or_default();
        seek_to(context, receiver(this.as_ref())?, target)
            .await
            .map(Value::Int)
    })
}

pub(super) fn on_rewind(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        rewind(context, object).await?;
        seek_to(context, object, limit(object)?.start).await?;
        Ok(Value::Null)
    })
}

pub(super) fn on_next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        next(context, object).await?;
        let position = read(object, |state| state.position)?;
        if !limit(object)?.beyond(position) {
            fetch(context, object).await?;
        }
        Ok(Value::Null)
    })
}

pub(super) fn position(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| Value::Int(state.position))
}
