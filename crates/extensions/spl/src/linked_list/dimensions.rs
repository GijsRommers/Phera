//! List dimensions follow the active direction and accept PHP's weak integer coercions.
//!
//! ```php
//! $stack[0]; // top of the stack
//! ```

use phera_extension_api::{
    Context, NativeDimensions, NativeError, NativeResult, Object, Param, ParamType, Value,
    prepare_call,
};

use super::{read, update};
use crate::{This, receiver};

pub(super) const OPERATIONS: NativeDimensions = NativeDimensions {
    direct: None,
    reference: None,
    bind: None,
    fallback_if_overridden: &[],
    get,
    exists,
    set,
    unset,
};

fn offset(context: &mut dyn Context, value: Value, method: &str) -> Result<i64, NativeError> {
    if let Value::Int(index) = value {
        return Ok(index);
    }
    let mut values = [value];
    let parameter = Param::new("index", ParamType::Int);
    let parameter = if method == "offsetSet" {
        parameter.nullable()
    } else {
        parameter
    };
    prepare_call(
        &format!("{}::{method}", super::CLASS),
        &[parameter],
        &mut values,
        context,
    )?;
    Ok(match values.first() {
        Some(Value::Int(index)) => *index,
        _ => 0,
    })
}

fn get(context: &mut dyn Context, object: &Object, index: Value) -> NativeResult {
    let index = offset(context, index, "offsetGet")?;
    read(object, |list| {
        let slot = list.slot(usize::try_from(index).ok()?, list.mode(object).order)?;
        list.entries
            .get(slot)
            .map(|entry| entry.value.deref().clone())
    })?
    .ok_or_else(|| super::range("offsetGet"))
}

fn exists(context: &mut dyn Context, object: &Object, index: Value) -> Result<bool, NativeError> {
    let index = offset(context, index, "offsetExists")?;
    read(object, |list| {
        usize::try_from(index).is_ok_and(|index| index < list.entries.len())
    })
}

fn set(
    context: &mut dyn Context,
    object: &Object,
    index: Value,
    value: Value,
) -> Result<(), NativeError> {
    let index = if index.is_null() {
        None
    } else {
        Some(offset(context, index, "offsetSet")?)
    };
    update(object, |list| {
        if let Some(index) = index {
            let slot = usize::try_from(index)
                .ok()
                .and_then(|index| list.slot(index, list.mode(object).order));
            let entry = slot
                .and_then(|slot| list.entries.get_mut(slot))
                .ok_or_else(|| super::range("offsetSet"))?;
            entry.value = value;
        } else {
            let entry = list.entry(value);
            list.entries.push_back(entry);
        }
        Ok(())
    })?
}

fn unset(context: &mut dyn Context, object: &Object, index: Value) -> Result<(), NativeError> {
    let index = offset(context, index, "offsetUnset")?;
    update(object, |list| {
        let slot = usize::try_from(index)
            .ok()
            .and_then(|index| list.slot(index, list.mode(object).order))
            .ok_or_else(|| super::range("offsetUnset"))?;
        list.entries.remove(slot);
        Ok(())
    })?
}

pub(super) fn offset_get(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    get(context, receiver(this)?, arg(args, 0))
}

pub(super) fn offset_exists(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    exists(context, receiver(this)?, arg(args, 0)).map(Value::Bool)
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

pub(super) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index)
        .map_or(Value::Null, |value| value.deref().clone())
}
