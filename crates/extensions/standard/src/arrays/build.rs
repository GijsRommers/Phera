//! Array construction and padding, including PHP's key stringification rules.

use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

const ARRAY_COMBINE_PARAMS: &[Param] = &[
    Param::new("keys", ParamType::Array),
    Param::new("values", ParamType::Array),
];

const ARRAY_FLIP_PARAMS: &[Param] = &[Param::new("array", ParamType::Array)];

const ARRAY_FILL_PARAMS: &[Param] = &[
    Param::new("start_index", ParamType::Int),
    Param::new("count", ParamType::Int),
    Param::new("value", ParamType::Mixed),
];

const ARRAY_FILL_KEYS_PARAMS: &[Param] = &[
    Param::new("keys", ParamType::Array),
    Param::new("value", ParamType::Mixed),
];

const ARRAY_PAD_PARAMS: &[Param] = &[
    Param::new("array", ParamType::Array),
    Param::new("length", ParamType::Int),
    Param::new("value", ParamType::Mixed),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, params, body) in [
        (
            "array_combine",
            ARRAY_COMBINE_PARAMS,
            array_combine as phera_extension_api::SyncFn,
        ),
        ("array_flip", ARRAY_FLIP_PARAMS, array_flip),
        ("array_fill", ARRAY_FILL_PARAMS, array_fill),
        ("array_fill_keys", ARRAY_FILL_KEYS_PARAMS, array_fill_keys),
        ("array_pad", ARRAY_PAD_PARAMS, array_pad),
    ] {
        registry.function(FunctionEntry::sync(name, params, body));
    }
}

/// Construction stringifies noninteger keys, unlike ordinary offset conversion.
pub(super) fn string_key(
    context: &mut dyn Context,
    value: &Value,
) -> Result<ArrayKey, NativeError> {
    if let Value::Int(integer) = &*value.deref() {
        return Ok(ArrayKey::Int(*integer));
    }
    Ok(ArrayKey::from_value(&Value::String(to_php_string(
        value,
        context.diagnostics(),
    )?))?)
}

/// Duplicate constructed keys overwrite values without moving their original positions.
fn array_combine(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    if let (Some(keys), Some(values)) = (args::array(arguments, 0), args::array(arguments, 1)) {
        if keys.len() != values.len() {
            return Err(NativeError::value_error(
                "array_combine(): Argument #1 ($keys) and argument #2 ($values) must have the same number of elements",
            ));
        }
        for (key, value) in keys.values().zip(values.values()) {
            result.insert(string_key(context, key)?, value.clone());
        }
    }
    Ok(Value::from(result))
}

/// Only integer and string values can become flipped keys; others emit warnings.
fn array_flip(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    if let Some(array) = args::array(arguments, 0) {
        for (key, value) in array.iter() {
            if matches!(&*value.deref(), Value::Int(_) | Value::String(_)) {
                result.insert(ArrayKey::from_value(value)?, super::key_value(key));
            } else {
                context.warning(
                    "array_flip(): Can only flip string and integer values, entry skipped".into(),
                );
            }
        }
    }
    Ok(Value::from(result))
}

/// Negative starting keys keep increasing from that key, as in PHP 8.
fn array_fill(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let count = args::int(arguments, 1).unwrap_or_default();
    if count < 0 {
        return Err(NativeError::value_error(
            "array_fill(): Argument #2 ($count) must be greater than or equal to 0",
        ));
    }
    if count > i64::from(i32::MAX) {
        return Err(NativeError::value_error(
            "array_fill(): Argument #2 ($count) is too large",
        ));
    }
    let start = args::int(arguments, 0).unwrap_or_default();
    if count > 0 && start.checked_add(count - 1).is_none() {
        return Err(NativeError::error(
            "Cannot add element to the array as the next element is already occupied",
        ));
    }
    let mut result = PhpArray::new();
    for offset in 0..count {
        if let Some(key) = start.checked_add(offset) {
            result.insert(ArrayKey::Int(key), args::value(arguments, 2).clone());
        }
    }
    Ok(Value::from(result))
}

/// Values become keys through string conversion, including bool and null values.
fn array_fill_keys(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    if let Some(keys) = args::array(arguments, 0) {
        for key in keys.values() {
            result.insert(string_key(context, key)?, args::value(arguments, 1).clone());
        }
    }
    Ok(Value::from(result))
}

/// Padding renumbers integer keys but returns an unchanged copy when already long enough.
fn array_pad(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let length = args::int(arguments, 1).unwrap_or_default();
    let count = length.unsigned_abs();
    if count > 1_073_741_824 {
        return Err(NativeError::value_error(
            "array_pad(): Argument #2 ($length) must not exceed the maximum allowed array size",
        ));
    }
    let Some(array) = args::array(arguments, 0) else {
        return Ok(Value::from(PhpArray::new()));
    };
    if count <= array.len() as u64 {
        return Ok(Value::Array(array.clone()));
    }
    let padding = count.saturating_sub(array.len() as u64);
    let mut result = PhpArray::new();
    if length < 0 {
        for _ in 0..padding {
            result.push(args::value(arguments, 2).clone())?;
        }
    }
    for (key, value) in array.iter() {
        super::slice::insert(&mut result, key, value, super::Keys::Renumber)?;
    }
    if length > 0 {
        for _ in 0..padding {
            result.push(args::value(arguments, 2).clone())?;
        }
    }
    Ok(Value::from(result))
}
