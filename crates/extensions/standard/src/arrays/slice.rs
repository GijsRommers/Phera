//! `array_slice([1, 2, 3, 4], 1, 2)` is `[2, 3]`; `array_reverse` too.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Value, args,
};

use super::{IntegerKeys, append_entry};
use crate::arguments::required_array;
use crate::offsets::clamped_range;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[ARRAY_SLICE, ARRAY_REVERSE];

/// `array_slice(array $array, int $offset, ?int $length = null, bool
/// $preserve_keys = false): array`
///
/// Offset and length count entries, not keys, and are clamped as in
/// `substr`. Integer keys are renumbered unless `$preserve_keys`; string
/// keys always stay.
///
/// ```php
/// array_slice([1, 2, 3, 4], 1, 2);              // [2, 3]
/// array_slice([1, 2, 3, 4], -2, null, true);    // [2 => 3, 3 => 4]
/// array_slice(["a" => 1, 5 => 2], 0);           // ["a" => 1, 0 => 2]
/// ```
const ARRAY_SLICE: FunctionEntry = FunctionEntry::sync(
    "array_slice",
    &[
        Param::new("array", ParamType::Array),
        Param::new("offset", ParamType::Int),
        Param::new("length", ParamType::Int).nullable().optional(),
        Param::new("preserve_keys", ParamType::Bool).optional(),
    ],
    array_slice,
);

fn array_slice(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let array = required_array(arguments, 0);
    let offset = args::int(arguments, 1).unwrap_or_default();
    let range = clamped_range(array.len(), offset, args::int(arguments, 2));
    let keys = IntegerKeys::preserve_if(args::bool(arguments, 3).unwrap_or(false));
    let mut result = PhpArray::new();
    if !range.is_empty()
        && let Some(position) = array.position_at(range.start)
    {
        for (key, value) in array.iter_from(position).take(range.len()) {
            append_entry(&mut result, key, value, keys)?;
        }
    }
    Ok(Value::from(result))
}

/// `array_reverse(array $array, bool $preserve_keys = false): array`
///
/// ```php
/// array_reverse([1, 2, "x" => 3]);       // ["x" => 3, 0 => 2, 1 => 1]
/// array_reverse([1, 2], true);           // [1 => 2, 0 => 1]
/// ```
const ARRAY_REVERSE: FunctionEntry = FunctionEntry::sync(
    "array_reverse",
    &[
        Param::new("array", ParamType::Array),
        Param::new("preserve_keys", ParamType::Bool).optional(),
    ],
    array_reverse,
);

fn array_reverse(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let array = required_array(arguments, 0);
    let keys = IntegerKeys::preserve_if(args::bool(arguments, 1).unwrap_or(false));
    // PhpArray::iter() cannot run backwards, so the entries are collected
    // first.
    let entries: Vec<_> = array.iter().collect();
    let mut result = PhpArray::new();
    for (key, value) in entries.into_iter().rev() {
        append_entry(&mut result, key, value, keys)?;
    }
    Ok(Value::from(result))
}
