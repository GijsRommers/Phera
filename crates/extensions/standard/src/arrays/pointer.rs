//! Internal array cursors move independently of key numbering.

use std::rc::Rc;

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

use super::key_value;

const READ: &[Param] = &[Param::new("array", ParamType::Array)];
const WRITE: &[Param] = &[Param::new("array", ParamType::Array).by_ref()];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("current", READ, current));
    registry.function(FunctionEntry::sync("pos", READ, current));
    registry.function(FunctionEntry::sync("key", READ, key));
    registry.function(FunctionEntry::sync("next", WRITE, next));
    registry.function(FunctionEntry::sync("prev", WRITE, prev));
    registry.function(FunctionEntry::sync("reset", WRITE, reset));
    registry.function(FunctionEntry::sync("end", WRITE, end));
}

fn current(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|array| array.current())
        .map_or(Value::Bool(false), |value| value.deref().clone()))
}

fn key(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(args::array(arguments, 0)
        .and_then(|array| array.key())
        .map_or(Value::Null, key_value))
}

/// Copy-on-write also isolates the cursor; a borrowed caller cell is a PHP Error.
fn move_pointer(arguments: &[Value], operation: fn(&mut PhpArray)) -> NativeResult {
    let cell = args::reference(arguments, 0)
        .ok_or_else(|| NativeError::error("Array argument must be passed by reference"))?;
    let mut value = cell
        .try_borrow_mut()
        .map_err(|_| NativeError::error("Cannot modify borrowed reference"))?;
    let Value::Array(array) = &mut *value else {
        return Err(NativeError::error("Expected prepared array argument"));
    };
    operation(Rc::make_mut(array));
    Ok(array
        .current()
        .map_or(Value::Bool(false), |value| value.deref().clone()))
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
