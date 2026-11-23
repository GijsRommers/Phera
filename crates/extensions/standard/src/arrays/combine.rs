//! Combining arrays: `array_merge`, `array_replace` and their recursive
//! forms. Merging appends integer keys and overwrites string keys;
//! replacing overwrites both.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Value,
};

use super::combine_recursive::combine_pair;
use super::{IntegerKeys, append_entry};
use crate::arguments::required_array;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    ARRAY_MERGE,
    ARRAY_REPLACE,
    ARRAY_MERGE_RECURSIVE,
    ARRAY_REPLACE_RECURSIVE,
];

/// What happens when two arrays have the same key.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Collision {
    /// `array_merge`: an integer key is appended under a new number; a
    /// string key is overwritten (or, recursively, merged).
    Merge,
    /// `array_replace`: any key is overwritten (or, recursively, replaced).
    Replace,
}

/// `array_merge(array ...$arrays): array`
///
/// ```php
/// array_merge([5 => "a", "x" => 1], [5 => "b", "x" => 2]); // [0 => "a", "x" => 2, 1 => "b"]
/// ```
const ARRAY_MERGE: FunctionEntry = FunctionEntry::sync(
    "array_merge",
    &[Param::new("arrays", ParamType::Array).variadic()],
    array_merge,
);

fn array_merge(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = PhpArray::new();
    for index in 0..arguments.len() {
        for (key, value) in required_array(arguments, index).iter() {
            append_entry(&mut result, key, value, IntegerKeys::Renumber)?;
        }
    }
    Ok(Value::from(result))
}

/// `array_replace(array $array, array ...$replacements): array`
///
/// ```php
/// array_replace([5 => "a", "x" => 1], [5 => "b"]); // [5 => "b", "x" => 1]
/// ```
const ARRAY_REPLACE: FunctionEntry = FunctionEntry::sync(
    "array_replace",
    &[
        Param::new("array", ParamType::Array),
        Param::new("replacements", ParamType::Array).variadic(),
    ],
    array_replace,
);

fn array_replace(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut result = (**required_array(arguments, 0)).clone();
    for index in 1..arguments.len() {
        for (key, value) in required_array(arguments, index).iter() {
            result.insert(key, value.clone());
        }
    }
    Ok(Value::from(result))
}

/// `array_merge_recursive(array ...$arrays): array`: values under the same
/// string key are merged into an array.
///
/// ```php
/// array_merge_recursive(["a" => 1], ["a" => 2]);     // ["a" => [1, 2]]
/// array_merge_recursive(["a" => [1]], ["a" => [2]]); // ["a" => [1, 2]]
/// ```
const ARRAY_MERGE_RECURSIVE: FunctionEntry = FunctionEntry::sync(
    "array_merge_recursive",
    &[Param::new("arrays", ParamType::Array).variadic()],
    array_merge_recursive,
);

fn array_merge_recursive(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    combine(context, arguments, Collision::Merge)
}

/// `array_replace_recursive(array $array, array ...$replacements): array`:
/// where both values are arrays, they are replaced into each other.
///
/// ```php
/// array_replace_recursive(["a" => [1, 2]], ["a" => [1 => "b"]]); // ["a" => [1, "b"]]
/// ```
const ARRAY_REPLACE_RECURSIVE: FunctionEntry = FunctionEntry::sync(
    "array_replace_recursive",
    &[
        Param::new("array", ParamType::Array),
        Param::new("replacements", ParamType::Array).variadic(),
    ],
    array_replace_recursive,
);

fn array_replace_recursive(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    combine(context, arguments, Collision::Replace)
}

fn combine(context: &mut dyn Context, arguments: &[Value], collision: Collision) -> NativeResult {
    // Replacing starts from a copy of the first array, which keeps its next
    // free integer key; merging starts empty and numbers from 0.
    let (mut result, sources) = match collision {
        Collision::Replace => ((**required_array(arguments, 0)).clone(), 1..arguments.len()),
        Collision::Merge => (PhpArray::new(), 0..arguments.len()),
    };
    for index in sources {
        let source = required_array(arguments, index).clone();
        result = combine_pair(context, result, source, collision)?;
    }
    Ok(Value::from(result))
}
