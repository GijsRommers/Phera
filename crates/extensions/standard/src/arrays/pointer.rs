//! The internal array pointer: `current`, `key`, `next`, `prev`, `reset`
//! and `end`.
//!
//! Every array has a position of its own, separate from `foreach`. A new
//! array, and one that was just sorted, points at its first entry; moving
//! past either end leaves the pointer nowhere, and `current()` is then
//! `false`.
//!
//! ```php
//! $a = [1, 2];
//! next($a);     // 2
//! next($a);     // false
//! reset($a);    // 1
//! ```

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Value,
};

use super::change_array_argument;
use crate::arguments::required_array;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[CURRENT, POS, KEY, NEXT, PREV, RESET, END];

/// The value at the pointer, or `false`.
fn current_value(array: &PhpArray) -> Value {
    array
        .current()
        .map_or(Value::Bool(false), |value| value.deref().clone())
}

/// `current(array|object $array): mixed`, the value at the pointer.
const CURRENT: FunctionEntry =
    FunctionEntry::sync("current", &[Param::new("array", ParamType::Array)], current);

/// `pos()`, another name for `current()`.
const POS: FunctionEntry =
    FunctionEntry::sync("pos", &[Param::new("array", ParamType::Array)], current);

fn current(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(current_value(required_array(arguments, 0)))
}

/// `key(array|object $array): int|string|null`, the key at the pointer.
const KEY: FunctionEntry =
    FunctionEntry::sync("key", &[Param::new("array", ParamType::Array)], key);

fn key(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let key = required_array(arguments, 0).key();
    Ok(key.map_or(Value::Null, Value::from))
}

/// `next(array|object &$array): mixed`, moving the pointer forward and
/// returning the value there.
const NEXT: FunctionEntry = FunctionEntry::sync(
    "next",
    &[Param::new("array", ParamType::Array).by_ref()],
    next,
);

fn next(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(move_pointer(arguments, PhpArray::advance))
}

/// `prev(array|object &$array): mixed`, moving the pointer back.
const PREV: FunctionEntry = FunctionEntry::sync(
    "prev",
    &[Param::new("array", ParamType::Array).by_ref()],
    prev,
);

fn prev(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(move_pointer(arguments, PhpArray::rewind_back))
}

/// `reset(array|object &$array): mixed`, the pointer to the first entry.
const RESET: FunctionEntry = FunctionEntry::sync(
    "reset",
    &[Param::new("array", ParamType::Array).by_ref()],
    reset,
);

fn reset(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(move_pointer(arguments, PhpArray::reset))
}

/// `end(array|object &$array): mixed`, the pointer to the last entry.
const END: FunctionEntry = FunctionEntry::sync(
    "end",
    &[Param::new("array", ParamType::Array).by_ref()],
    end,
);

fn end(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(move_pointer(arguments, PhpArray::end))
}

/// Moves the pointer of the caller's array and returns the value there.
fn move_pointer(arguments: &[Value], movement: fn(&mut PhpArray)) -> Value {
    change_array_argument(arguments, |array| {
        movement(array);
        current_value(array)
    })
}
