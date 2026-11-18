//! A direct offsetExists call detects null entries; isset answers false for those entries.
//!
//! ```php
//! $a = new ArrayObject(['x' => null]);
//! var_dump($a->offsetExists('x'), isset($a['x'])); // true, false
//! ```

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{
    ArrayKey, Context, DimensionReferenceMode, NativeDimensions, NativeError, NativeResult, Object,
    Value,
};

use super::state::{self, Target};
use crate::{This, receiver};

pub(super) const HANDLERS: NativeDimensions = NativeDimensions {
    fallback_if_overridden: &[],
    get,
    exists,
    set,
    unset,
    reference: Some(reference),
    bind: Some(bind),
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
    properties: bool,
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
    let key = ArrayKey::from_value_diag(&value, context.diagnostics())?;
    Ok(match key {
        ArrayKey::Int(key) if properties => ArrayKey::str(key.to_string()),
        _ => key,
    })
}

pub(super) fn get(context: &mut dyn Context, object: &Object, offset: Value) -> NativeResult {
    let target = Target::of(object)?;
    let key = key(
        context,
        object,
        &offset,
        Access::Read,
        target.is_properties(),
    )?;
    let found = target.read(|array| array.get(&key).cloned())?;
    Ok(match found {
        Some(value) => value.deref().clone(),
        None => {
            warn_missing(context, &key);
            Value::Null
        }
    })
}

fn warn_missing(context: &mut dyn Context, key: &ArrayKey) {
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
}

fn exists(context: &mut dyn Context, object: &Object, offset: Value) -> Result<bool, NativeError> {
    let target = Target::of(object)?;
    let key = key(
        context,
        object,
        &offset,
        Access::Exists,
        target.is_properties(),
    )?;
    target.read(|array| {
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
    let target = Target::of(object)?;
    let key = if matches!(&*offset.deref(), Value::Null) {
        None
    } else {
        Some(key(
            context,
            object,
            &offset,
            Access::Read,
            target.is_properties(),
        )?)
    };
    target.update(|array| {
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
    let target = Target::of(object)?;
    let key = key(
        context,
        object,
        &offset,
        Access::Unset,
        target.is_properties(),
    )?;
    target.update(|array| {
        array.remove(&key);
        Ok(())
    })
}

pub(super) fn element_reference(slot: &mut Value) -> Value {
    if !slot.is_reference() {
        *slot = Value::Reference(Rc::new(RefCell::new(std::mem::take(slot))));
    }
    slot.clone()
}

fn reference(
    context: &mut dyn Context,
    object: &Object,
    offset: Value,
    mode: DimensionReferenceMode,
) -> NativeResult {
    let target = Target::of(object)?;
    let key = key(
        context,
        object,
        &offset,
        Access::Read,
        target.is_properties(),
    )?;
    let found = target.read(|array| array.contains_key(&key))?;
    if !found {
        if mode == DimensionReferenceMode::Unset {
            return Ok(Value::Null);
        }
        if mode == DimensionReferenceMode::ReadWrite {
            warn_missing(context, &key);
        }
    }
    target.update(|array| {
        if !found {
            array.insert(key.clone(), Value::Null);
        }
        array
            .get_mut(&key)
            .map(element_reference)
            .ok_or_else(|| NativeError::error("SPL array entry vanished"))
    })
}

fn bind(
    context: &mut dyn Context,
    object: &Object,
    offset: Value,
    value: Value,
) -> Result<(), NativeError> {
    let target = Target::of(object)?;
    let append = offset.is_null();
    let key = (!append)
        .then(|| {
            key(
                context,
                object,
                &offset,
                Access::Read,
                target.is_properties(),
            )
        })
        .transpose()?;
    target.update(|array| {
        let mut value = value;
        let value = element_reference(&mut value);
        if let Some(key) = key {
            array.insert(key, value);
        } else {
            let _ = array.push(value);
        }
        Ok(())
    })
}

pub(super) fn offset_exists(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let object = receiver(this)?;
    let target = Target::of(object)?;
    let key = key(
        context,
        object,
        &arg(args, 0),
        Access::Exists,
        target.is_properties(),
    )?;
    Ok(Value::Bool(target.read(|array| array.contains_key(&key))?))
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
