//! By-reference stack, queue, and splice operations with copy-on-write arrays.
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};
use std::rc::Rc;
const ONE: &[Param] = &[Param::new("array", ParamType::Array).by_ref()];
const PUSH: &[Param] = &[
    Param::new("array", ParamType::Array).by_ref(),
    Param::new("values", ParamType::Mixed).variadic(),
];
const SPLICE: &[Param] = &[
    Param::new("array", ParamType::Array).by_ref(),
    Param::new("offset", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
    Param::new("replacement", ParamType::Mixed).optional(),
];
pub(crate) fn register(registry: &mut Registry) {
    for (name, params, body) in [
        ("array_push", PUSH, push as phera_extension_api::SyncFn),
        ("array_pop", ONE, pop),
        ("array_shift", ONE, shift),
        ("array_unshift", PUSH, unshift),
        ("array_splice", SPLICE, splice),
    ] {
        registry.function(FunctionEntry::sync(name, params, body));
    }
}
/// Reference cells may be borrowed already; report an Error instead of panicking.
pub(super) fn mutate(
    arguments: &[Value],
    operation: impl FnOnce(&mut PhpArray) -> NativeResult,
) -> NativeResult {
    let cell = args::reference(arguments, 0)
        .ok_or_else(|| NativeError::error("Expected array reference"))?;
    let mut value = cell
        .try_borrow_mut()
        .map_err(|_| NativeError::error("Array is already being modified"))?;
    let Value::Array(array) = &mut *value else {
        return Err(NativeError::error("Expected array"));
    };
    operation(Rc::make_mut(array))
}
fn push(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    mutate(arguments, |array| {
        for value in arguments.iter().skip(1) {
            array.push(value.clone())?;
        }
        Ok(Value::Int(array.len() as i64))
    })
}
fn pop(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    mutate(arguments, |array| {
        let result = array.pop().unwrap_or(Value::Null);
        array.reset();
        Ok(result.deref().clone())
    })
}
fn shift(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    mutate(arguments, |array| {
        Ok(array.shift().unwrap_or(Value::Null).deref().clone())
    })
}
fn unshift(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    mutate(arguments, |array| {
        let mut result = PhpArray::new();
        for value in arguments.iter().skip(1) {
            result.push(value.clone())?;
        }
        for (key, value) in array.iter() {
            super::slice::insert(&mut result, key, value, false)?;
        }
        result.reset();
        *array = result;
        Ok(Value::Int(array.len() as i64))
    })
}
/// Splice retains string keys of survivors and removed entries, but drops replacement keys.
fn splice(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let replacement =
        phera_extension_api::value::to_array(args::value(arguments, 3), context.diagnostics());
    mutate(arguments, |array| {
        let offset = args::int(arguments, 1).unwrap_or_default();
        let start = if offset < 0 {
            array.len().saturating_sub(offset.unsigned_abs() as usize)
        } else {
            (offset as usize).min(array.len())
        };
        let end = match args::int(arguments, 2) {
            None => array.len(),
            Some(length) if length < 0 => array
                .len()
                .saturating_sub(length.unsigned_abs() as usize)
                .max(start),
            Some(length) => start.saturating_add(length as usize).min(array.len()),
        };
        let mut removed = PhpArray::new();
        let mut result = PhpArray::new();
        for (key, value) in array.iter().take(start) {
            super::slice::insert(&mut result, key, value, false)?;
        }
        for value in replacement.values() {
            result.push(value.clone())?;
        }
        for (index, (key, value)) in array.iter().enumerate().skip(start) {
            super::slice::insert(
                if index < end {
                    &mut removed
                } else {
                    &mut result
                },
                key,
                value,
                false,
            )?;
        }
        result.reset();
        *array = result;
        Ok(Value::from(removed))
    })
}
