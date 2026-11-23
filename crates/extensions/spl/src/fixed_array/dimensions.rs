//! SplFixedArray supports integer indices but has no append operation.
//!
//! ```php
//! unset($a[0]); // sets the existing slot to null
//! ```

use phera_extension_api::{
    ArrayKey, Context, NativeDimensions, NativeError, NativeResult, Object, Value, args,
};

use super::{CLASS, read, update};
use crate::{This, receiver};

pub(super) const OPERATIONS: NativeDimensions = NativeDimensions {
    direct: None,
    reference: None,
    bind: None,
    fallback_if_overridden: &[],
    get,
    set,
    unset,
    exists,
};

fn index(context: &mut dyn Context, index: &Value) -> Result<Option<usize>, NativeError> {
    let value = index.deref();
    if matches!(
        &*value,
        Value::Int(_) | Value::Bool(_) | Value::Float(_) | Value::InternedStr(_) | Value::String(_)
    ) && let ArrayKey::Int(index) = ArrayKey::from_value_diag(&value, context.diagnostics())?
    {
        return Ok(usize::try_from(index).ok());
    }
    Err(NativeError::type_error(format!(
        "Cannot access offset of type {} on {CLASS}",
        value.type_name(),
    )))
}

fn out_of_bounds() -> NativeError {
    NativeError::throw("OutOfBoundsException", "Index invalid or out of range")
}

fn get(context: &mut dyn Context, object: &Object, key: Value) -> NativeResult {
    let index = index(context, &key)?.ok_or_else(out_of_bounds)?;
    read(object, |fixed| fixed.slots.get(index).cloned())?.ok_or_else(out_of_bounds)
}

fn exists(context: &mut dyn Context, object: &Object, key: Value) -> Result<bool, NativeError> {
    let Some(index) = index(context, &key)? else {
        return Ok(false);
    };
    read(object, |fixed| {
        fixed.slots.get(index).is_some_and(|value| !value.is_null())
    })
}

fn set(
    context: &mut dyn Context,
    object: &Object,
    key: Value,
    value: Value,
) -> Result<(), NativeError> {
    if key.is_null() {
        return Err(NativeError::error(
            "[] operator not supported for SplFixedArray",
        ));
    }
    write(context, object, key, value)
}

fn write(
    context: &mut dyn Context,
    object: &Object,
    key: Value,
    value: Value,
) -> Result<(), NativeError> {
    let index = index(context, &key)?.ok_or_else(out_of_bounds)?;
    update(object, |fixed| {
        let slot = fixed.slots.get_mut(index).ok_or_else(out_of_bounds)?;
        *slot = value.deref().clone();
        Ok(())
    })?
}

fn unset(context: &mut dyn Context, object: &Object, key: Value) -> Result<(), NativeError> {
    write(context, object, key, Value::Null)
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
    write(
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
