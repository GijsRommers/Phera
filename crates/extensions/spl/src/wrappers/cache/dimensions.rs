//! A full caching iterator allows array access to already visited keys.
//!
//! ```php
//! $cached['last'] = 'value';
//! var_dump($cached->offsetExists('last'));
//! ```

use phera_extension_api::{
    ArrayKey, Context, NativeDimensions, NativeError, NativeResult, Object, Param, ParamType,
    Value, args, prepare_call,
};

use super::{CLASS, cache, change};
use crate::{This, receiver};

pub(super) const DIMENSIONS: NativeDimensions = NativeDimensions {
    reference: None,
    bind: None,
    fallback_if_overridden: &[],
    get,
    exists,
    set,
    unset,
};

fn key(
    context: &mut dyn Context,
    value: Value,
    method: &str,
) -> Result<(ArrayKey, Value), NativeError> {
    let mut arguments = [value];
    prepare_call(
        &format!("{CLASS}::{method}"),
        &[Param::new("key", ParamType::String)],
        &mut arguments,
        context,
    )?;
    let [value] = arguments;
    Ok((
        ArrayKey::from_value_diag(&value, context.diagnostics())?,
        value,
    ))
}

fn require(object: &Object) -> Result<(), NativeError> {
    if cache(object, |cache| cache.flags.full_cache())? {
        return Ok(());
    }
    Err(NativeError::throw(
        "BadMethodCallException",
        format!(
            "{} does not use a full cache (see CachingIterator::__construct)",
            object.class.name()
        ),
    ))
}

fn get(context: &mut dyn Context, object: &Object, index: Value) -> NativeResult {
    let (key, value) = key(context, index, "offsetGet")?;
    require(object)?;
    let entry = cache(object, |cache| cache.rows.get(&key).cloned())?;
    Ok(entry.unwrap_or_else(|| {
        if let Value::String(value) = value {
            crate::warn_string_key(context, value.as_bytes());
        }
        Value::Null
    }))
}

fn exists(context: &mut dyn Context, object: &Object, index: Value) -> Result<bool, NativeError> {
    let (key, _) = key(context, index, "offsetExists")?;
    require(object)?;
    cache(object, |cache| cache.rows.contains_key(&key))
}

fn set(
    context: &mut dyn Context,
    object: &Object,
    index: Value,
    value: Value,
) -> Result<(), NativeError> {
    let (key, _) = key(context, index, "offsetSet")?;
    require(object)?;
    change(object, |cache| {
        cache.rows.insert(key, value.deref().clone());
    })
}

fn unset(context: &mut dyn Context, object: &Object, index: Value) -> Result<(), NativeError> {
    let (key, _) = key(context, index, "offsetUnset")?;
    require(object)?;
    change(object, |cache| {
        cache.rows.remove(&key);
    })
}

pub(super) fn offset_get(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    get(context, receiver(this)?, args::value(args, 0).clone())
}

pub(super) fn offset_exists(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    exists(context, receiver(this)?, args::value(args, 0).clone()).map(Value::Bool)
}

pub(super) fn offset_set(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    set(
        context,
        receiver(this)?,
        args::value(args, 0).clone(),
        args::value(args, 1).clone(),
    )?;
    Ok(Value::Null)
}

pub(super) fn offset_unset(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    unset(context, receiver(this)?, args::value(args, 0).clone())?;
    Ok(Value::Null)
}

pub(super) fn get_cache(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    require(object)?;
    cache(object, |cache| Value::from(cache.rows.clone()))
}

pub(super) fn native_count(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    require(object)?;
    cache(object, |cache| cache.rows.len() as i64)
}

pub(super) fn count(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    native_count(context, receiver(this)?).map(Value::Int)
}
