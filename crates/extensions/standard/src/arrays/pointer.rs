//! Internal array cursors move independently of key numbering.
//!
//! ```php
//! $a = [1]; $a[] = &$a;
//! end($a) === $a; // true
//! ```

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value, args,
};

use super::key_value;

const ARRAY_READ_PARAMS: &[Param] = &[Param::new("array", ParamType::Array)];

const ARRAY_WRITE_PARAMS: &[Param] = &[Param::new("array", ParamType::Array).by_ref()];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("current", ARRAY_READ_PARAMS, current));
    registry.function(FunctionEntry::sync("pos", ARRAY_READ_PARAMS, current));
    registry.function(FunctionEntry::sync("key", ARRAY_READ_PARAMS, key));
    registry.function(FunctionEntry::sync("next", ARRAY_WRITE_PARAMS, next));
    registry.function(FunctionEntry::sync("prev", ARRAY_WRITE_PARAMS, prev));
    registry.function(FunctionEntry::sync("reset", ARRAY_WRITE_PARAMS, reset));
    registry.function(FunctionEntry::sync("end", ARRAY_WRITE_PARAMS, end));
}

fn current(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    match args::array(arguments, 0).and_then(|array| array.current()) {
        Some(value) => Ok(value.try_deref()?.clone()),
        None => Ok(Value::Bool(false)),
    }
}

fn key(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|array| array.key())
        .map_or(Value::Null, |key| key_value(&key)))
}

/// Copy-on-write also isolates the cursor; a borrowed caller cell is a PHP Error.
fn move_pointer(arguments: &[Value], operation: fn(&mut PhpArray)) -> NativeResult {
    let pointed = super::stack::mutate_with_errors(
        arguments,
        |array| {
            operation(array);
            Ok(array.current().cloned().unwrap_or(Value::Bool(false)))
        },
        [
            "Array argument must be passed by reference",
            "Cannot modify borrowed reference",
            "Expected prepared array argument",
        ],
    )?;
    Ok(pointed.try_deref()?.clone())
}

fn next(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    move_pointer(arguments, PhpArray::advance)
}

fn prev(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    move_pointer(arguments, PhpArray::rewind_back)
}

fn reset(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    move_pointer(arguments, PhpArray::reset)
}

fn end(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    move_pointer(arguments, PhpArray::end)
}
