//! Keys, values and lookups: `array_keys`, `in_array`, `array_search`,
//! `array_key_exists`, `array_first` and their relatives.

use phera_extension_api::value::{loose_equals, strict_equals};
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    PhpString, Value, args,
};

use super::list;
use crate::arguments::required_array;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    ARRAY_VALUES,
    ARRAY_KEYS,
    ARRAY_KEY_FIRST,
    ARRAY_KEY_LAST,
    ARRAY_FIRST,
    ARRAY_LAST,
    ARRAY_IS_LIST,
    IN_ARRAY,
    ARRAY_SEARCH,
    ARRAY_KEY_EXISTS,
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
    let values = required_array(arguments, 0).values().cloned();
    Ok(Value::from(list(values)))
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
const ARRAY_KEYS: FunctionEntry = FunctionEntry::sync(
    "array_keys",
    &[
        Param::new("array", ParamType::Array),
        Param::new("filter_value", ParamType::Mixed).optional(),
        Param::new("strict", ParamType::Bool).optional(),
    ],
    array_keys,
);

fn array_keys(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let array = required_array(arguments, 0);
    let Some(filter) = arguments.get(1) else {
        return Ok(Value::from(list(array.keys().map(Value::from))));
    };
    let equality = Equality::from_strict_argument(arguments);
    let mut result = PhpArray::new();
    for (key, value) in array.iter() {
        if equality.equal(context, value, filter)? {
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

/// How `in_array`, `array_search` and `array_keys` compare.
#[derive(Clone, Copy)]
enum Equality {
    /// `==`, with PHP's type juggling.
    Loose,
    /// `===`.
    Strict,
}

impl Equality {
    /// From the `$strict` argument, which is the third in all three.
    fn from_strict_argument(arguments: &[Value]) -> Self {
        if args::bool(arguments, 2).unwrap_or(false) {
            Self::Strict
        } else {
            Self::Loose
        }
    }

    fn equal(
        self,
        context: &mut dyn Context,
        left: &Value,
        right: &Value,
    ) -> Result<bool, NativeError> {
        let result = match self {
            Self::Strict => strict_equals(left, right, context.diagnostics())?,
            Self::Loose => loose_equals(left, right, context.diagnostics())?,
        };
        Ok(matches!(result, Value::Bool(true)))
    }
}

/// `in_array(mixed $needle, array $haystack, bool $strict = false): bool`
///
/// ```php
/// in_array("1e1", [10]);       // true: "1e1" == 10
/// in_array("1e1", [10], true); // false
/// ```
const IN_ARRAY: FunctionEntry = FunctionEntry::sync(
    "in_array",
    &[
        Param::new("needle", ParamType::Mixed),
        Param::new("haystack", ParamType::Array),
        Param::new("strict", ParamType::Bool).optional(),
    ],
    in_array,
);

fn in_array(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(search(context, arguments)?.is_some()))
}

/// `array_search(mixed $needle, array $haystack, bool $strict = false):
/// int|string|false`, the first key whose value equals the needle.
///
/// ```php
/// array_search("b", ["x" => "a", "y" => "b"]); // "y"
/// array_search("c", ["a"]);                    // false
/// ```
const ARRAY_SEARCH: FunctionEntry = FunctionEntry::sync(
    "array_search",
    &[
        Param::new("needle", ParamType::Mixed),
        Param::new("haystack", ParamType::Array),
        Param::new("strict", ParamType::Bool).optional(),
    ],
    array_search,
);

fn array_search(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let found = search(context, arguments)?;
    Ok(found.map_or(Value::Bool(false), Value::from))
}

/// The first key of `$haystack` whose value equals `$needle`.
fn search(context: &mut dyn Context, arguments: &[Value]) -> Result<Option<ArrayKey>, NativeError> {
    let needle = &arguments[0];
    let equality = Equality::from_strict_argument(arguments);
    for (key, value) in required_array(arguments, 1).iter() {
        if equality.equal(context, needle, value)? {
            return Ok(Some(key));
        }
    }
    Ok(None)
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
    let key = &arguments[0];
    let key = if key.is_null() {
        context.deprecated(
            "Using null as the key parameter for array_key_exists() is deprecated, use an empty string instead"
                .into(),
        );
        ArrayKey::String(PhpString::new())
    } else {
        ArrayKey::from_value_diag(key, context.diagnostics())?
    };
    Ok(Value::Bool(required_array(arguments, 1).contains_key(&key)))
}
