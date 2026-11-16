//! Slice operations for PHP values.

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "array_slice",
        const {
            &[
                Param::new("array", ParamType::Array),
                Param::new("offset", ParamType::Int),
                Param::new("length", ParamType::Int).nullable().optional(),
                Param::new("preserve_keys", ParamType::Bool).optional(),
            ]
        },
        slice,
    ));
    registry.function(FunctionEntry::sync(
        "array_reverse",
        const {
            &[
                Param::new("array", ParamType::Array),
                Param::new("preserve_keys", ParamType::Bool).optional(),
            ]
        },
        reverse,
    ));
}

/// Slice and reverse renumber integer keys unless asked to preserve them.
fn insert(
    result: &mut PhpArray,
    key: &ArrayKey,
    value: &Value,
    preserve: bool,
) -> Result<(), NativeError> {
    if preserve || matches!(key, ArrayKey::String(_)) {
        result.insert(key.clone(), value.clone());
    } else {
        result.push(value.clone())?;
    }
    Ok(())
}

/// Negative lengths count from the end; string keys always survive slicing.
fn slice(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        let offset = args::int(arguments, 1).unwrap_or_default();
        let start = if offset < 0 {
            array.len().saturating_sub(offset.unsigned_abs() as usize)
        } else {
            (offset as usize).min(array.len())
        };
        let end = match args::int(arguments, 2) {
            None => array.len(),
            Some(n) if n < 0 => array
                .len()
                .saturating_sub(n.unsigned_abs() as usize)
                .max(start),
            Some(n) => start.saturating_add(n as usize).min(array.len()),
        };
        for (key, value) in array.iter().skip(start).take(end - start) {
            insert(
                &mut result,
                key,
                value,
                args::bool(arguments, 3).unwrap_or(false),
            )?;
        }
    }
    Ok(Value::Array(Rc::new(result)))
}

/// Reversal changes iteration order while preserving string-key identity.
fn reverse(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        for (key, value) in array.iter().collect::<Vec<_>>().into_iter().rev() {
            insert(
                &mut result,
                key,
                value,
                args::bool(arguments, 1).unwrap_or(false),
            )?;
        }
    }
    Ok(Value::Array(Rc::new(result)))
}
