//! The outer iterator exposes the current depth and each active inner iterator.
//!
//! ```php
//! $it->setMaxDepth(3);
//! echo $it->getDepth();
//! ```

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, Value, args, traversal::IteratorStep,
};
use std::rc::Rc;

use super::{
    Mode, Settings, State, at, attach, depth, inner, read, step, uninitialized, update, walk,
};
use crate::{This, receiver, store, wrappers::actual_iterator};

pub(super) fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let iterator = args::object(&arguments, 0)
            .ok_or_else(uninitialized)?
            .clone();
        let iterator = actual_iterator(context, iterator).await?;
        let mode = args::int(&arguments, 1).unwrap_or_default();
        let flags = args::int(&arguments, 2).unwrap_or_default();
        build(context, object, iterator, mode, flags, None).await
    })
}

pub(super) async fn build(
    context: &mut dyn Context,
    object: &Rc<Object>,
    iterator: Rc<Object>,
    mode: i64,
    flags: i64,
    tree: Option<super::tree::Tree>,
) -> NativeResult {
    if !iterator.class.is_instance_of("RecursiveIterator") {
        return Err(NativeError::throw(
            "InvalidArgumentException",
            "An instance of RecursiveIterator or IteratorAggregate creating it is required",
        ));
    }
    store(
        object,
        State {
            settings: Some(Settings {
                mode: Mode::parse(mode),
                catch_children: flags & 16 != 0,
            }),
            tree,
            ..State::default()
        },
    )?;
    attach(context, object, iterator).await?;
    Ok(Value::Null)
}

pub(super) fn rewind(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { walk::run(context, receiver(this.as_ref())?, true).await })
}

pub(super) fn next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { walk::run(context, receiver(this.as_ref())?, false).await })
}

pub(super) fn valid(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { walk::valid(context, receiver(this.as_ref())?).await })
}

pub(super) async fn read_current(
    context: &mut dyn Context,
    object: &Object,
    operation: IteratorStep,
) -> NativeResult {
    let level = depth(object)?;
    if at(object, level)?.is_none() {
        return Err(uninitialized());
    }
    step(context, object, level, operation).await
}

pub(super) fn current(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(
        async move { read_current(context, receiver(this.as_ref())?, IteratorStep::Current).await },
    )
}

pub(super) fn key(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(
        async move { read_current(context, receiver(this.as_ref())?, IteratorStep::Key).await },
    )
}

pub(super) fn get_depth(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    depth(receiver(this)?).map(|depth| Value::Int(depth as i64))
}

pub(super) fn get_sub(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let level = match args::int(arguments, 0) {
        Some(level) => usize::try_from(level).ok(),
        None => Some(depth(object)?),
    };
    let Some(level) = level else {
        return Ok(Value::Null);
    };
    Ok(at(object, level)?.map_or(Value::Null, Value::Object))
}

pub(super) fn get_inner(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    inner(receiver(this)?).map(Value::Object)
}

pub(super) fn get_max(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    read(receiver(this)?, |state| {
        state.max_depth.map_or(Value::Bool(false), Value::Int)
    })
}

pub(super) fn set_max(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let max = args::int(arguments, 0).unwrap_or(-1);
    if max < -1 {
        return Err(NativeError::value_error(
            "RecursiveIteratorIterator::setMaxDepth(): Argument #1 ($maxDepth) must be greater than or equal to -1",
        ));
    }
    update(receiver(this)?, |state| {
        state.max_depth = (max >= 0).then_some(max.min(i64::from(i32::MAX)));
        Value::Null
    })
}

pub(super) fn has_children(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { walk::has_children(context, receiver(this.as_ref())?, false).await })
}

pub(super) fn children(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { walk::children(context, receiver(this.as_ref())?, false).await })
}

pub(super) fn no_action(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Null)
}
