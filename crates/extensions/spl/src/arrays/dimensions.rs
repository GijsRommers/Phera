//! A direct offsetExists call detects null entries; isset answers false for those entries.
//!
//! ```php
//! $a = new ArrayObject(['x' => null]);
//! var_dump($a->offsetExists('x'), isset($a['x'])); // true, false
//! ```

use phera_extension_api::{
    ArrayKey, Context, NativeDimensions, NativeError, NativeResult, Object, Value,
};

use super::state::{self, Target};
use crate::{This, receiver};

pub(super) const HANDLERS: NativeDimensions = NativeDimensions {
    fallback_if_overridden: &[],
    get,
    exists,
    set,
    unset,
};

enum Access {
    Read,
    Exists,
    Unset,
}

fn key(
    context: &mut dyn Context,
    object: &Object,
    offset: &Value,
    access: Access,
) -> Result<ArrayKey, NativeError> {
    let value = offset.deref();
    if matches!(&*value, Value::Object(_) | Value::Array(_)) {
        let kind = value.type_name();
        let class = object.class.name();
        return Err(NativeError::type_error(match access {
            Access::Read => format!("Cannot access offset of type {kind} on {class}"),
            Access::Exists => format!("Cannot access offset of type {kind} in isset or empty"),
            Access::Unset => format!("Cannot unset offset of type {kind} on {class}"),
        }));
    }
    Ok(ArrayKey::from_value_diag(&value, context.diagnostics())?)
}

pub(super) fn get(context: &mut dyn Context, object: &Object, offset: Value) -> NativeResult {
    let key = key(context, object, &offset, Access::Read)?;
    let found = Target::of(object)?.read(|array| array.get(&key).cloned())?;
    Ok(match found {
        Some(value) => value.deref().clone(),
        None => {
            let key = match key {
                ArrayKey::Int(key) => key.to_string(),
                ArrayKey::String(key) => {
                    let bytes = key.as_bytes();
                    let length = bytes
                        .iter()
                        .position(|byte| *byte == 0)
                        .unwrap_or(bytes.len());
                    format!("\"{}\"", String::from_utf8_lossy(&bytes[..length]))
                }
            };
            context
                .diagnostics()
                .warning(format!("Undefined array key {key}"));
            Value::Null
        }
    })
}

fn exists(context: &mut dyn Context, object: &Object, offset: Value) -> Result<bool, NativeError> {
    let key = key(context, object, &offset, Access::Exists)?;
    Target::of(object)?.read(|array| {
        array
            .get(&key)
            .is_some_and(|value| !matches!(&*value.deref(), Value::Null))
    })
}

pub(super) fn set(
    context: &mut dyn Context,
    object: &Object,
    offset: Value,
    value: Value,
) -> Result<(), NativeError> {
    state::check_mutable(object)?;
    let key = if matches!(&*offset.deref(), Value::Null) {
        None
    } else {
        Some(key(context, object, &offset, Access::Read)?)
    };
    Target::of(object)?.update(|array| {
        let value = value.deref().clone();
        if let Some(key) = key {
            array.insert(key, value);
        } else {
            // SPL silently ignores an append when PHP_INT_MAX is already occupied.
            let _ = array.push(value);
        }
        Ok(())
    })
}

pub(super) fn unset(
    context: &mut dyn Context,
    object: &Object,
    offset: Value,
) -> Result<(), NativeError> {
    state::check_mutable(object)?;
    let key = key(context, object, &offset, Access::Unset)?;
    Target::of(object)?.update(|array| {
        array.remove(&key);
        Ok(())
    })
}

pub(super) fn offset_exists(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let object = receiver(this)?;
    let key = key(context, object, &arg(args, 0), Access::Exists)?;
    Ok(Value::Bool(
        Target::of(object)?.read(|array| array.contains_key(&key))?,
    ))
}

pub(super) fn offset_get(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    get(context, receiver(this)?, arg(args, 0))
}

pub(super) fn offset_set(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    set(context, receiver(this)?, arg(args, 0), arg(args, 1))?;
    Ok(Value::Null)
}

pub(super) fn offset_unset(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    unset(context, receiver(this)?, arg(args, 0))?;
    Ok(Value::Null)
}

pub(super) fn arg(arguments: &[Value], index: usize) -> Value {
    arguments
        .get(index)
        .map_or(Value::Null, |value| value.deref().clone())
}
