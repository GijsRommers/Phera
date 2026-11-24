//! Sets of entries: `array_diff`, `array_intersect` and their `_key` and
//! `_assoc` forms, and `array_unique`. They keep the keys of the first
//! array and compare values as strings: `1`, `"1"` and `true` are equal.

use foldhash::{HashSet, HashSetExt};
use std::cmp::Ordering;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray, Value, args,
};

use super::ordering::SortFlags;
use super::zend_sort;
use crate::arguments::{required_array, stringify};
use crate::constants::SORT_STRING;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    ARRAY_DIFF,
    ARRAY_DIFF_KEY,
    ARRAY_DIFF_ASSOC,
    ARRAY_INTERSECT,
    ARRAY_INTERSECT_KEY,
    ARRAY_INTERSECT_ASSOC,
    ARRAY_UNIQUE,
];

/// `array_diff(array $array, array ...$arrays): array`, the entries whose
/// value is in none of the other arrays.
///
/// ```php
/// array_diff(["a" => 1, "b" => 2, "c" => 3], [2], ["3"]); // ["a" => 1]
/// ```
const ARRAY_DIFF: FunctionEntry = FunctionEntry::asynchronous(
    "array_diff",
    &[
        Param::new("array", ParamType::Array),
        Param::new("arrays", ParamType::Array).variadic(),
    ],
    array_diff,
);

/// Like php_array_diff, this converts each value to a string once and looks
/// it up, instead of comparing pairs; that decides which conversions (and
/// their warnings) happen.
fn array_diff(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        difference(context, &arguments)
            .await
            .map_err(|error| context.native_call_error("array_diff", &arguments, error))
    })
}

async fn difference(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let array = required_array(arguments, 0);
    if array.is_empty() {
        return Ok(arguments[0].clone());
    }
    // One value: PHP compares it with the others' values one by one, and
    // stops at the first that is equal.
    if let (1, Some(only)) = (array.len(), array.values().next()) {
        let only = stringify(context, only).await?;
        for other in other_arrays(arguments) {
            for value in other.values() {
                if only == stringify(context, value).await? {
                    return Ok(Value::from(PhpArray::new()));
                }
            }
        }
        return Ok(arguments[0].clone());
    }
    if arguments.len() == 1 {
        return Ok(arguments[0].clone());
    }
    let mut excluded = HashSet::new();
    for other in other_arrays(arguments) {
        for value in other.values() {
            excluded.insert(stringify(context, value).await?);
        }
    }
    if excluded.is_empty() {
        return Ok(arguments[0].clone());
    }
    let mut result = PhpArray::new();
    for (key, value) in array.iter() {
        if !excluded.contains(&stringify(context, value).await?) {
            result.insert(key, value.clone());
        }
    }
    Ok(Value::from(result))
}

/// The arrays after the first, which every set function compares with.
fn other_arrays(arguments: &[Value]) -> impl Iterator<Item = &PhpArray> {
    (1..arguments.len()).map(|index| &**required_array(arguments, index))
}

/// `array_diff_key(array $array, array ...$arrays): array`, the entries
/// whose key is in none of the other arrays.
///
/// ```php
/// array_diff_key(["a" => 1, "b" => 2], ["a" => 9]); // ["b" => 2]
/// ```
const ARRAY_DIFF_KEY: FunctionEntry = FunctionEntry::sync(
    "array_diff_key",
    &[
        Param::new("array", ParamType::Array),
        Param::new("arrays", ParamType::Array).variadic(),
    ],
    array_diff_key,
);

fn array_diff_key(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    keep_keys(arguments, Keep::InNone)
}

/// `array_diff_assoc(array $array, array ...$arrays): array`, the entries
/// that no other array has with the same key and value.
///
/// ```php
/// array_diff_assoc(["a" => 1, "b" => 2], ["a" => "1", "b" => 3]); // ["b" => 2]
/// ```
const ARRAY_DIFF_ASSOC: FunctionEntry = FunctionEntry::asynchronous(
    "array_diff_assoc",
    &[
        Param::new("array", ParamType::Array),
        Param::new("arrays", ParamType::Array).variadic(),
    ],
    super::callback_sets::diff_associative_strings,
);

/// `array_intersect(array $array, array ...$arrays): array`, the entries
/// whose value is in every other array.
///
/// ```php
/// array_intersect(["a" => 1, "b" => 2], [2, 3]); // ["b" => 2]
/// ```
const ARRAY_INTERSECT: FunctionEntry = FunctionEntry::asynchronous(
    "array_intersect",
    &[
        Param::new("array", ParamType::Array),
        Param::new("arrays", ParamType::Array).variadic(),
    ],
    super::callback_sets::intersect_strings,
);

/// `array_intersect_key(array $array, array ...$arrays): array`, the
/// entries whose key is in every other array.
const ARRAY_INTERSECT_KEY: FunctionEntry = FunctionEntry::sync(
    "array_intersect_key",
    &[
        Param::new("array", ParamType::Array),
        Param::new("arrays", ParamType::Array).variadic(),
    ],
    array_intersect_key,
);

fn array_intersect_key(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    keep_keys(arguments, Keep::InAll)
}

/// `array_intersect_assoc(array $array, array ...$arrays): array`, the
/// entries that every other array has with the same key and value.
const ARRAY_INTERSECT_ASSOC: FunctionEntry = FunctionEntry::asynchronous(
    "array_intersect_assoc",
    &[
        Param::new("array", ParamType::Array),
        Param::new("arrays", ParamType::Array).variadic(),
    ],
    super::callback_sets::intersect_associative_strings,
);

/// Which entries of the first array a set function keeps.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Keep {
    /// Those found in none of the other arrays (`array_diff…`).
    InNone,
    /// Those found in all of them (`array_intersect…`).
    InAll,
}

fn keep_keys(arguments: &[Value], keep: Keep) -> NativeResult {
    let mut result = PhpArray::new();
    'entries: for (key, value) in required_array(arguments, 0).iter() {
        for other in other_arrays(arguments) {
            if other.contains_key(&key) != (keep == Keep::InAll) {
                continue 'entries;
            }
        }
        result.insert(key, value.clone());
    }
    Ok(Value::from(result))
}

/// `array_unique(array $array, int $flags = SORT_STRING): array`, the
/// first entry of each value, with its key.
///
/// With `SORT_STRING`, values are compared as strings through a lookup;
/// with any other flag, PHP sorts and compares neighbours, which is why
/// that path goes through zend_sort too.
///
/// ```php
/// array_unique(["a" => 1, "b" => "1", "c" => 2]); // ["a" => 1, "c" => 2]
/// array_unique([4, "4", "3", 4, 3], SORT_REGULAR); // [0 => 4, 2 => "3"]
/// ```
const ARRAY_UNIQUE: FunctionEntry = FunctionEntry::asynchronous(
    "array_unique",
    &[
        Param::new("array", ParamType::Array),
        Param::new("flags", ParamType::Int).optional(),
    ],
    array_unique,
);

fn array_unique(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        unique(context, &arguments)
            .await
            .map_err(|error| context.native_call_error("array_unique", &arguments, error))
    })
}

async fn unique(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let flags = args::int(arguments, 1).unwrap_or(SORT_STRING);
    let array = required_array(arguments, 0);
    if array.len() <= 1 {
        return Ok(arguments[0].clone());
    }
    if flags == SORT_STRING {
        let mut seen = HashSet::new();
        let mut result = PhpArray::new();
        for (key, value) in array.iter() {
            if seen.insert(stringify(context, value).await?) {
                result.insert(key, value.clone());
            }
        }
        return Ok(Value::from(result));
    }
    let flags = SortFlags::from_flags(flags);
    let entries: Vec<_> = array.iter().collect();
    let order = zend_sort::order_async(entries.len(), async |left, right| {
        flags
            .compare_unstable(context, entries[left].1, entries[right].1)
            .await
    })
    .await?;
    // Walk the sorted entries; of a run of equal values, the one that came
    // first in the array stays.
    let mut result = (**array).clone();
    let mut kept: Option<usize> = None;
    for index in order {
        if let Some(last) = kept
            && flags
                .compare_unstable(context, entries[last].1, entries[index].1)
                .await?
                == Ordering::Equal
        {
            if last > index {
                result.remove(&entries[last].0);
                kept = Some(index);
            } else {
                result.remove(&entries[index].0);
            }
            continue;
        }
        kept = Some(index);
    }
    Ok(Value::from(result))
}
