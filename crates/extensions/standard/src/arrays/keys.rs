//! Keys, values and lookups: `array_keys`, `in_array`, `array_search`,
//! `array_key_exists`, `array_first` and their relatives.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray,
    PhpString, Value, args,
};

use super::search;
use crate::arguments::required_array;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    ARRAY_VALUES,
    ARRAY_KEYS,
    ARRAY_KEY_FIRST,
    ARRAY_KEY_LAST,
    ARRAY_FIRST,
    ARRAY_LAST,
    ARRAY_IS_LIST,
    search::IN_ARRAY,
    search::ARRAY_SEARCH,
    ARRAY_KEY_EXISTS,
    ARRAY_CHANGE_KEY_CASE,
    KEY_EXISTS,
];

/// `array_values(array $array): array`, the values as a list.
///
/// ```php
/// array_values(["a" => 1, 5 => 2]); // [1, 2]
/// ```
const ARRAY_VALUES: FunctionEntry = FunctionEntry::sync(
    "array_values",
    &[Param::new("array", ParamType::Array)],
    array_values,
);

fn array_values(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let array = required_array(arguments, 0);
    Ok(Value::from(PhpArray::from_values(
        array.values().cloned(),
        array.len(),
    )))
}

/// `array_keys(array $array, mixed $filter_value, bool $strict = false):
/// array`, the keys as a list, or only those whose value equals
/// `$filter_value` (with `==`, or `===` when strict).
///
/// ```php
/// array_keys(["a" => 1, "b" => "1", "c" => 2]);          // ["a", "b", "c"]
/// array_keys(["a" => 1, "b" => "1", "c" => 2], 1);       // ["a", "b"]
/// array_keys(["a" => 1, "b" => "1", "c" => 2], 1, true); // ["a"]
/// ```
const ARRAY_KEYS: FunctionEntry = FunctionEntry::asynchronous(
    "array_keys",
    &[
        Param::new("array", ParamType::Array),
        Param::new("filter_value", ParamType::Mixed).optional(),
        Param::new("strict", ParamType::Bool).optional(),
    ],
    array_keys,
);

fn array_keys(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(collect_keys(context, arguments))
}

async fn collect_keys(context: &mut dyn Context, arguments: Vec<Value>) -> NativeResult {
    let arguments = &arguments;
    let array = required_array(arguments, 0);
    let Some(filter) = arguments.get(1) else {
        return Ok(Value::from(PhpArray::from_values(
            array.keys().map(Value::from),
            array.len(),
        )));
    };
    let equality = search::Equality::from_strict_argument(arguments);
    let mut result = PhpArray::new();
    for (key, value) in array.iter() {
        // The filter is the left operand, which matters for a recursive array.
        if equality.equal(context, filter, value).await? {
            result.push(Value::from(key))?;
        }
    }
    Ok(Value::from(result))
}

/// `array_key_first(array $array): int|string|null`
///
/// ```php
/// array_key_first(["x" => 1, "y" => 2]); // "x"
/// array_key_first([]);                   // null
/// ```
const ARRAY_KEY_FIRST: FunctionEntry = FunctionEntry::sync(
    "array_key_first",
    &[Param::new("array", ParamType::Array)],
    array_key_first,
);

fn array_key_first(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let first = required_array(arguments, 0).first();
    Ok(first.map_or(Value::Null, |(key, _)| Value::from(key)))
}

/// `array_key_last(array $array): int|string|null`
const ARRAY_KEY_LAST: FunctionEntry = FunctionEntry::sync(
    "array_key_last",
    &[Param::new("array", ParamType::Array)],
    array_key_last,
);

fn array_key_last(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let last = required_array(arguments, 0).last();
    Ok(last.map_or(Value::Null, |(key, _)| Value::from(key)))
}

/// `array_first(array $array): mixed`, new in PHP 8.5; null for `[]`.
///
/// ```php
/// array_first(["x" => 1, "y" => 2]); // 1
/// ```
const ARRAY_FIRST: FunctionEntry = FunctionEntry::sync(
    "array_first",
    &[Param::new("array", ParamType::Array)],
    array_first,
);

fn array_first(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let first = required_array(arguments, 0).first();
    Ok(first.map_or(Value::Null, |(_, value)| value.clone()))
}

/// `array_last(array $array): mixed`, new in PHP 8.5; null for `[]`.
const ARRAY_LAST: FunctionEntry = FunctionEntry::sync(
    "array_last",
    &[Param::new("array", ParamType::Array)],
    array_last,
);

fn array_last(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let last = required_array(arguments, 0).last();
    Ok(last.map_or(Value::Null, |(_, value)| value.clone()))
}

/// `array_is_list(array $array): bool`, whether the keys are 0, 1, 2… in
/// order.
///
/// ```php
/// array_is_list([1, 2]);        // true
/// array_is_list([1 => 1, 0 => 2]); // false
/// ```
const ARRAY_IS_LIST: FunctionEntry = FunctionEntry::sync(
    "array_is_list",
    &[Param::new("array", ParamType::Array)],
    array_is_list,
);

fn array_is_list(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(required_array(arguments, 0).is_list()))
}

/// `array_key_exists(string|int|float|bool|resource|null $key, array
/// $array): bool`, true also when the value is null, unlike `isset`.
///
/// The key is converted as in `$array[$key]`; a null key is the empty
/// string, which PHP 8.5 deprecates.
///
/// ```php
/// array_key_exists("a", ["a" => null]); // true
/// array_key_exists("1", [1 => "x"]);    // true
/// ```
const ARRAY_KEY_EXISTS: FunctionEntry = FunctionEntry::sync(
    "array_key_exists",
    &[
        Param::new("key", ParamType::Mixed),
        Param::new("array", ParamType::Array),
    ],
    array_key_exists,
);

/// `key_exists()`, another name for `array_key_exists()`.
const KEY_EXISTS: FunctionEntry = FunctionEntry::sync(
    "key_exists",
    &[
        Param::new("key", ParamType::Mixed),
        Param::new("array", ParamType::Array),
    ],
    array_key_exists,
);

fn array_key_exists(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let array = required_array(arguments, 1);
    let exists = match &*arguments[0].deref() {
        Value::Int(key) => array.contains_int(*key),
        Value::String(key) => array.contains_str(key),
        Value::Null => {
            context.deprecated(
                "Using null as the key parameter for array_key_exists() is deprecated, use an empty string instead"
                    .into(),
            );
            array.contains_key(&ArrayKey::String(PhpString::new()))
        }
        other => array.contains_key(&ArrayKey::from_value_diag(other, context.diagnostics())?),
    };
    Ok(Value::Bool(exists))
}

/// `array_change_key_case(array $array, int $case = CASE_LOWER): array`,
/// the array with its string keys in lower or upper case. Only the letters
/// a to z change (PHP 8.2 and later); integer keys stay as they are.
///
/// ```php
/// array_change_key_case(["Id" => 1, "NAME" => "x"]);             // ["id" => 1, "name" => "x"]
/// array_change_key_case(["Id" => 1, "ID" => 2], CASE_UPPER);     // ["ID" => 2]
/// ```
///
/// Two keys that become the same leave one entry, at the place of the
/// first and with the value of the last.
const ARRAY_CHANGE_KEY_CASE: FunctionEntry = FunctionEntry::sync(
    "array_change_key_case",
    &[
        Param::new("array", ParamType::Array),
        Param::new("case", ParamType::Int).optional(),
    ],
    array_change_key_case,
);

fn array_change_key_case(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    // CASE_LOWER is 0; PHP takes every other number as CASE_UPPER.
    let upper = args::int(arguments, 1).is_some_and(|case| case != 0);
    let mut result = PhpArray::new();
    for (key, value) in required_array(arguments, 0).iter() {
        let key = match key {
            ArrayKey::String(name) if upper => {
                ArrayKey::String(PhpString::from(name.as_bytes().to_ascii_uppercase()))
            }
            ArrayKey::String(name) => {
                ArrayKey::String(PhpString::from(name.as_bytes().to_ascii_lowercase()))
            }
            ArrayKey::Int(_) => key.clone(),
        };
        result.insert(key, value.clone());
    }
    Ok(Value::from(result))
}
