//! Building arrays from other values: `array_combine`, `array_flip`,
//! `array_fill`, `array_fill_keys` and `array_pad`.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType,
    PhpArray, Value, args,
};

use super::{IntegerKeys, append_entry};
use crate::arguments::{required_array, stringify};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    ARRAY_COMBINE,
    ARRAY_FLIP,
    ARRAY_FILL,
    ARRAY_FILL_KEYS,
    ARRAY_PAD,
];

/// The most entries `array_pad` makes, PHP's `HT_MAX_SIZE` on 64 bits.
const MAX_ARRAY_SIZE: u64 = 1 << 30;

/// A value as a key the way `array_combine` and `array_fill_keys` read it:
/// an int stays an int, anything else becomes a string first, so `true` is
/// `"1"` and `1.5` is `"1.5"` (where `$array[1.5]` would be 1).
async fn key_from_string(
    context: &mut dyn Context,
    value: &Value,
) -> Result<ArrayKey, NativeError> {
    if let Value::Int(integer) = &*value.deref() {
        return Ok(ArrayKey::Int(*integer));
    }
    let string = stringify(context, value).await?;
    Ok(ArrayKey::from_value(&Value::String(string))?)
}

/// `array_combine(array $keys, array $values): array`
///
/// A key that comes twice keeps its first place and its last value.
///
/// ```php
/// array_combine(["a", "b"], [1, 2]);  // ["a" => 1, "b" => 2]
/// array_combine(["a", "a"], [1, 2]);  // ["a" => 2]
/// array_combine(["a"], [1, 2]);       // ValueError
/// ```
const ARRAY_COMBINE: FunctionEntry = FunctionEntry::asynchronous(
    "array_combine",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("values", ParamType::Array),
    ],
    array_combine,
);

fn array_combine(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        combine(context, &arguments)
            .await
            .map_err(|error| context.native_call_error("array_combine", &arguments, error))
    })
}

async fn combine(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let keys = required_array(arguments, 0);
    let values = required_array(arguments, 1);
    if keys.len() != values.len() {
        return Err(NativeError::value_error(
            "array_combine(): Argument #1 ($keys) and argument #2 ($values) must have the same number of elements",
        ));
    }
    let mut result = PhpArray::new();
    for (key, value) in keys.values().zip(values.values()) {
        result.insert(key_from_string(context, key).await?, value.clone());
    }
    Ok(Value::from(result))
}

/// `array_flip(array $array): array`, keys and values swapped. Values that
/// cannot be keys are skipped with a warning.
///
/// ```php
/// array_flip(["a" => 1, "b" => 2]); // [1 => "a", 2 => "b"]
/// ```
const ARRAY_FLIP: FunctionEntry = FunctionEntry::sync(
    "array_flip",
    &[Param::new("array", ParamType::Array)],
    array_flip,
);

fn array_flip(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    for (key, value) in required_array(arguments, 0).iter() {
        if matches!(&*value.deref(), Value::Int(_) | Value::String(_)) {
            result.insert(ArrayKey::from_value(value)?, Value::from(key));
        } else {
            context.warning(
                "array_flip(): Can only flip string and integer values, entry skipped".into(),
            );
        }
    }
    Ok(Value::from(result))
}

/// `array_fill(int $start_index, int $count, mixed $value): array`
///
/// ```php
/// array_fill(5, 2, "x");  // [5 => "x", 6 => "x"]
/// array_fill(-3, 2, "x"); // [-3 => "x", -2 => "x"] since PHP 8
/// ```
const ARRAY_FILL: FunctionEntry = FunctionEntry::sync(
    "array_fill",
    &[
        Param::new("start_index", ParamType::Int),
        Param::new("count", ParamType::Int),
        Param::new("value", ParamType::Mixed),
    ],
    array_fill,
);

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
    let value = &arguments[2];
    let mut result = PhpArray::new();
    for offset in 0..count {
        result.insert(ArrayKey::Int(start + offset), value.clone());
    }
    Ok(Value::from(result))
}

/// `array_fill_keys(array $keys, mixed $value): array`
///
/// ```php
/// array_fill_keys(["a", 5], 0); // ["a" => 0, 5 => 0]
/// ```
const ARRAY_FILL_KEYS: FunctionEntry = FunctionEntry::asynchronous(
    "array_fill_keys",
    &[
        Param::new("keys", ParamType::Array),
        Param::new("value", ParamType::Mixed),
    ],
    array_fill_keys,
);

fn array_fill_keys(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        fill_keys(context, &arguments)
            .await
            .map_err(|error| context.native_call_error("array_fill_keys", &arguments, error))
    })
}

async fn fill_keys(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let value = &arguments[1];
    let mut result = PhpArray::new();
    for key in required_array(arguments, 0).values() {
        result.insert(key_from_string(context, key).await?, value.clone());
    }
    Ok(Value::from(result))
}

/// `array_pad(array $array, int $length, mixed $value): array`, padded with
/// `$value` to `$length` entries: at the end, or at the start when the
/// length is negative. Integer keys are numbered again; an array that is
/// long enough comes back as it is.
///
/// ```php
/// array_pad([1, 2], 4, 0);  // [1, 2, 0, 0]
/// array_pad([1, 2], -4, 0); // [0, 0, 1, 2]
/// ```
const ARRAY_PAD: FunctionEntry = FunctionEntry::sync(
    "array_pad",
    &[
        Param::new("array", ParamType::Array),
        Param::new("length", ParamType::Int),
        Param::new("value", ParamType::Mixed),
    ],
    array_pad,
);

fn array_pad(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let length = args::int(arguments, 1).unwrap_or_default();
    let wanted = length.unsigned_abs();
    if wanted > MAX_ARRAY_SIZE {
        return Err(NativeError::value_error(
            "array_pad(): Argument #2 ($length) must not exceed the maximum allowed array size",
        ));
    }
    let array = required_array(arguments, 0);
    if wanted <= array.len() as u64 {
        return Ok(Value::Array(array.clone()));
    }
    let padding = wanted - array.len() as u64;
    let value = &arguments[2];
    let mut result = PhpArray::new();
    if length < 0 {
        for _ in 0..padding {
            result.push(value.clone())?;
        }
    }
    for (key, entry) in array.iter() {
        append_entry(&mut result, key, entry, IntegerKeys::Renumber)?;
    }
    if length > 0 {
        for _ in 0..padding {
            result.push(value.clone())?;
        }
    }
    Ok(Value::from(result))
}
