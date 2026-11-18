//! Changing an array in place: `array_push`, `array_pop`, `array_shift`,
//! `array_unshift` and `array_splice`.

use phera_extension_api::value::to_array;
use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Value, args,
};

use super::{IntegerKeys, append_entry, change_array_argument};
use crate::offsets::clamped_range;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    ARRAY_PUSH,
    ARRAY_POP,
    ARRAY_SHIFT,
    ARRAY_UNSHIFT,
    ARRAY_SPLICE,
];

/// `array_push(array &$array, mixed ...$values): int`, the new length.
///
/// ```php
/// $a = [1];
/// array_push($a, 2, 3); // 3, and $a is [1, 2, 3]
/// ```
const ARRAY_PUSH: FunctionEntry = FunctionEntry::sync(
    "array_push",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("values", ParamType::Mixed).variadic(),
    ],
    array_push,
);

fn array_push(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    change_array_argument(arguments, |array| {
        for value in &arguments[1..] {
            array.push(value.clone())?;
        }
        Ok(Value::Int(array.len() as i64))
    })
}

/// `array_pop(array &$array): mixed`, the last value removed, or null; the
/// pointer goes back to the start.
///
/// ```php
/// $a = [1, 2];
/// array_pop($a); // 2, and $a is [1]
/// ```
const ARRAY_POP: FunctionEntry = FunctionEntry::sync(
    "array_pop",
    &[Param::new("array", ParamType::Array).by_ref()],
    array_pop,
);

fn array_pop(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    change_array_argument(arguments, |array| {
        let last = array.pop().unwrap_or(Value::Null);
        array.reset();
        Ok(last.deref().clone())
    })
}

/// `array_shift(array &$array): mixed`, the first value removed, or null;
/// integer keys are numbered again from 0.
///
/// ```php
/// $a = [5 => "a", 9 => "b"];
/// array_shift($a); // "a", and $a is [0 => "b"]
/// ```
const ARRAY_SHIFT: FunctionEntry = FunctionEntry::sync(
    "array_shift",
    &[Param::new("array", ParamType::Array).by_ref()],
    array_shift,
);

fn array_shift(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    change_array_argument(arguments, |array| {
        Ok(array.shift().unwrap_or(Value::Null).deref().clone())
    })
}

/// `array_unshift(array &$array, mixed ...$values): int`, the values put in
/// front; integer keys are numbered again, string keys stay.
///
/// ```php
/// $a = ["x" => 1, 5 => 2];
/// array_unshift($a, 0); // 3, and $a is [0 => 0, "x" => 1, 1 => 2]
/// ```
const ARRAY_UNSHIFT: FunctionEntry = FunctionEntry::sync(
    "array_unshift",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("values", ParamType::Mixed).variadic(),
    ],
    array_unshift,
);

fn array_unshift(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    change_array_argument(arguments, |array| {
        let mut result = PhpArray::new();
        for value in &arguments[1..] {
            result.push(value.clone())?;
        }
        for (key, value) in array.iter() {
            append_entry(&mut result, key, value, IntegerKeys::Renumber)?;
        }
        result.reset();
        *array = result;
        Ok(Value::Int(array.len() as i64))
    })
}

/// `array_splice(array &$array, int $offset, ?int $length = null, mixed
/// $replacement = []): array`, the entries that `$offset` and `$length`
/// select (as in `array_slice`) replaced and returned.
///
/// Integer keys are numbered again, in the array and in the result; the
/// replacement's keys are dropped; a replacement that is no array is
/// one value.
///
/// ```php
/// $a = ["a", "b", "c", "d"];
/// array_splice($a, 1, 2, "X"); // ["b", "c"], and $a is ["a", "X", "d"]
/// ```
const ARRAY_SPLICE: FunctionEntry = FunctionEntry::sync(
    "array_splice",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("offset", ParamType::Int),
        Param::new("length", ParamType::Int).nullable().optional(),
        Param::new("replacement", ParamType::Mixed).optional(),
    ],
    array_splice,
);

fn array_splice(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let replacement = to_array(args::value(arguments, 3), context.diagnostics());
    change_array_argument(arguments, |array| {
        let offset = args::int(arguments, 1).unwrap_or_default();
        let removed_range = clamped_range(array.len(), offset, args::int(arguments, 2));
        let mut removed = PhpArray::new();
        let mut kept = PhpArray::new();
        for (key, value) in array.iter().take(removed_range.start) {
            append_entry(&mut kept, key, value, IntegerKeys::Renumber)?;
        }
        for value in replacement.values() {
            kept.push(value.clone())?;
        }
        for (index, (key, value)) in array.iter().enumerate().skip(removed_range.start) {
            let target = if index < removed_range.end {
                &mut removed
            } else {
                &mut kept
            };
            append_entry(target, key, value, IntegerKeys::Renumber)?;
        }
        kept.reset();
        *array = kept;
        Ok(Value::from(removed))
    })
}
