//! Iterator adapters hold their inner traversal and the currently cached row.
//!
//! ```php
//! $iterator = new IteratorIterator(new ArrayObject(['a' => 1]));
//! $iterator->rewind();
//! echo $iterator->key(); // a
//! ```

mod append;
pub(crate) mod cache;
mod filter;
mod limit;
mod methods;
mod regex;
mod registration;

pub(crate) use registration::register;

use std::rc::Rc;

use phera_extension_api::{
    Context, NativeError, NativeResult, Object, Value,
    traversal::{self, IteratorStep, Traversal},
};

#[derive(Default)]
struct Wrapper {
    inner: Option<Rc<Object>>,
    cursor: Option<Box<dyn Traversal>>,
    value: Option<Value>,
    key: Value,
    position: i64,
    callback: Option<Value>,
    limit: Option<limit::Limit>,
    cache: Option<cache::Cache>,
    append: Option<append::Append>,
    regex: Option<regex::Regex>,
}

fn uninitialized() -> NativeError {
    NativeError::error("The object is in an invalid state as the parent constructor was not called")
}

fn read<T>(object: &Object, f: impl FnOnce(&Wrapper) -> T) -> Result<T, NativeError> {
    crate::read_state(object, |state: &Wrapper| {
        if state.inner.is_none() && state.append.is_none() {
            return Err(uninitialized());
        }
        Ok(f(state))
    })?
}

fn update<T>(object: &Object, f: impl FnOnce(&mut Wrapper) -> T) -> Result<T, NativeError> {
    crate::update_state(object, |state: &mut Wrapper| {
        if state.inner.is_none() && state.append.is_none() {
            return Err(uninitialized());
        }
        Ok(f(state))
    })?
}

async fn new_wrapper(
    context: &mut dyn Context,
    object: &Rc<Object>,
    inner: Rc<Object>,
    mut state: Wrapper,
) -> NativeResult {
    if read(object, |_| ()).is_ok() {
        return Err(NativeError::throw(
            "BadMethodCallException",
            format!(
                "{}::getIterator() must be called exactly once per instance",
                object.class.name()
            ),
        ));
    }
    let iterator = actual_iterator(context, inner).await?;
    if Rc::ptr_eq(&iterator, object) {
        return Err(uninitialized());
    }
    state.cursor = Some(context.traverse(iterator.clone()).await?);
    state.inner = Some(iterator);
    crate::store(object, state)?;
    Ok(Value::Null)
}

pub(crate) async fn actual_iterator(
    context: &mut dyn Context,
    mut iterator: Rc<Object>,
) -> Result<Rc<Object>, NativeError> {
    let mut seen = std::collections::HashSet::new();
    while !iterator.class.is_instance_of("Iterator") {
        if !iterator.class.is_instance_of("IteratorAggregate") {
            return Ok(iterator);
        }
        if !seen.insert(iterator.id) {
            return Err(NativeError::throw(
                "LogicException",
                "IteratorAggregate returned itself",
            ));
        }
        let result = context
            .call_method(iterator.clone(), "getIterator", Vec::new())
            .await?;
        match &*result.deref() {
            Value::Object(object) if object.class.is_traversable() => iterator = object.clone(),
            _ => {
                return Err(NativeError::throw(
                    "Exception",
                    format!(
                        "Objects returned by {}::getIterator() must be traversable or implement interface Iterator",
                        iterator.class.name(),
                    ),
                ));
            }
        };
    }
    Ok(iterator)
}

async fn step(context: &mut dyn Context, object: &Object, operation: IteratorStep) -> NativeResult {
    let mut cursor = update(object, |state| state.cursor.take())?
        .ok_or_else(|| NativeError::error("Cannot re-enter an active SPL iterator"))?;
    let result = traversal::step(cursor.as_mut(), context, operation).await;
    update(object, |state| state.cursor = Some(cursor))?;
    result
}

fn inner(object: &Object) -> Result<Rc<Object>, NativeError> {
    read(object, |state| state.inner.clone())?.ok_or_else(uninitialized)
}

fn forward_method(object: &Object) -> Result<Option<Rc<Object>>, NativeError> {
    Ok(inner(object).ok())
}

fn clear(object: &Object) -> Result<(), NativeError> {
    update(object, |state| {
        state.value = None;
        state.key = Value::Null;
    })
}

async fn fetch(context: &mut dyn Context, object: &Object) -> Result<bool, NativeError> {
    clear(object)?;
    if !phera_extension_api::value::to_bool(&step(context, object, IteratorStep::Valid).await?) {
        return Ok(false);
    }
    let value = step(context, object, IteratorStep::Current).await?;
    let key = step(context, object, IteratorStep::Key).await?;
    update(object, |state| {
        state.value = Some(value);
        state.key = key;
    })?;
    Ok(true)
}

async fn rewind(context: &mut dyn Context, object: &Object) -> Result<(), NativeError> {
    clear(object)?;
    update(object, |state| state.position = 0)?;
    step(context, object, IteratorStep::Rewind).await?;
    Ok(())
}

async fn next(context: &mut dyn Context, object: &Object) -> Result<(), NativeError> {
    clear(object)?;
    step(context, object, IteratorStep::Advance).await?;
    update(object, |state| {
        state.position = state.position.saturating_add(1)
    })
}

async fn source(
    context: &mut dyn Context,
    object: &Rc<Object>,
    arguments: &[Value],
    callback: Option<Value>,
) -> NativeResult {
    let inner = phera_extension_api::args::object(arguments, 0).ok_or_else(uninitialized)?;
    new_wrapper(
        context,
        object,
        inner.clone(),
        Wrapper {
            callback,
            ..Wrapper::default()
        },
    )
    .await
}
