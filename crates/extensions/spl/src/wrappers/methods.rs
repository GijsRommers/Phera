//! IteratorIterator snapshots the inner current/key after rewind or next.
//!
//! ```php
//! $it = new NoRewindIterator($inner);
//! $it->rewind(); // leaves the inner cursor in place
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, Value, args, traversal::IteratorStep,
};

use super::{fetch, next, read, rewind, source, step};
use crate::{This, receiver};

pub(super) fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { source(context, receiver(this.as_ref())?, &arguments, None).await })
}

pub(super) fn construct_base(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        if let Some(name) = args::string(&arguments, 1) {
            let name = name.to_string_lossy();
            if args::object(&arguments, 0).is_some_and(|o| !o.class.is_instance_of("Iterator"))
                && context.lookup_class(&name).await?.is_none()
            {
                return Err(NativeError::throw(
                    "LogicException",
                    format!("Class {name} does not exist"),
                ));
            }
        }
        source(context, receiver(this.as_ref())?, &arguments, None).await
    })
}

pub(super) fn get_inner(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| {
        state.inner.clone().map_or(Value::Null, Value::Object)
    })
}

pub(super) fn current(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| {
        state.value.clone().unwrap_or(Value::Null)
    })
}

pub(super) fn key(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| state.key.clone())
}

pub(super) fn valid(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| Value::Bool(state.value.is_some()))
}

pub(super) fn on_rewind(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        rewind(context, object).await?;
        fetch(context, object).await?;
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
        fetch(context, object).await?;
        Ok(Value::Null)
    })
}

pub(super) fn infinite_next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        next(context, object).await?;
        if !fetch(context, object).await? {
            rewind(context, object).await?;
            fetch(context, object).await?;
        }
        Ok(Value::Null)
    })
}

pub(super) fn no_action(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Null)
}

pub(super) fn empty_current(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    Err(NativeError::throw(
        "BadMethodCallException",
        "Accessing the value of an EmptyIterator",
    ))
}

pub(super) fn empty_key(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    Err(NativeError::throw(
        "BadMethodCallException",
        "Accessing the key of an EmptyIterator",
    ))
}

pub(super) fn empty_valid(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(false))
}

pub(super) fn no_rewind_valid(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let result = step(context, receiver(this.as_ref())?, IteratorStep::Valid).await?;
        Ok(Value::Bool(phera_extension_api::value::to_bool(&result)))
    })
}

pub(super) fn no_rewind_current(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { step(context, receiver(this.as_ref())?, IteratorStep::Current).await })
}

pub(super) fn no_rewind_key(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { step(context, receiver(this.as_ref())?, IteratorStep::Key).await })
}

pub(super) fn no_rewind_next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        step(context, receiver(this.as_ref())?, IteratorStep::Advance).await?;
        Ok(Value::Null)
    })
}
