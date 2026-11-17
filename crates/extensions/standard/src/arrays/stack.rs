//! By-reference stack, queue, and splice operations with copy-on-write arrays.
//!
//! ```php
//! $a = [1]; $a[] = &$a;
//! array_pop($a); // [1], because the removed reference follows the modified array
//! ```

use std::rc::Rc;

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

const ARRAY_ONE_PARAMS: &[Param] = &[Param::new("array", ParamType::Array).by_ref()];

const ARRAY_PUSH_PARAMS: &[Param] = &[
    Param::new("array", ParamType::Array).by_ref(),
    Param::new("values", ParamType::Mixed).variadic(),
];

const ARRAY_SPLICE_PARAMS: &[Param] = &[
    Param::new("array", ParamType::Array).by_ref(),
    Param::new("offset", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
    Param::new("replacement", ParamType::Mixed).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, params, body) in [
        (
            "array_push",
            ARRAY_PUSH_PARAMS,
            array_push as phera_extension_api::SyncFn,
        ),
        ("array_pop", ARRAY_ONE_PARAMS, array_pop),
        ("array_shift", ARRAY_ONE_PARAMS, array_shift),
        ("array_unshift", ARRAY_PUSH_PARAMS, array_unshift),
        ("array_splice", ARRAY_SPLICE_PARAMS, array_splice),
    ] {
        registry.function(FunctionEntry::sync(name, params, body));
    }
}

/// Reference cells may be borrowed already; report an Error instead of panicking.
pub(super) fn mutate(
    arguments: &[Value],
    operation: impl FnOnce(&mut PhpArray) -> NativeResult,
) -> NativeResult {
    mutate_with_errors(
        arguments,
        operation,
        [
            "Expected array reference",
            "Array is already being modified",
            "Expected array",
        ],
    )
}

/// Borrow conflicts become PHP errors; callers retain their own diagnostic wording.
pub(super) fn mutate_with_errors(
    arguments: &[Value],
    operation: impl FnOnce(&mut PhpArray) -> NativeResult,
    [missing_reference, borrowed, wrong_type]: [&str; 3],
) -> NativeResult {
    let cell =
        args::reference(arguments, 0).ok_or_else(|| NativeError::error(missing_reference))?;
    let mut value = cell
        .try_borrow_mut()
        .map_err(|_| NativeError::error(borrowed))?;
    let Value::Array(array) = &mut *value else {
        return Err(NativeError::error(wrong_type));
    };
    operation(Rc::make_mut(array))
}

fn array_push(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    mutate(arguments, |array| {
        for value in arguments.iter().skip(1) {
            array.push(value.clone())?;
        }
        Ok(Value::Int(array.len() as i64))
    })
}

/// Popping resets the internal cursor and returns the dereferenced removed value.
fn array_pop(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let removed = mutate(arguments, |array| {
        let removed = array.pop().unwrap_or(Value::Null);
        array.reset();
        Ok(removed)
    })?;
    Ok(removed.try_deref()?.clone())
}

/// Shifting renumbers integer keys and returns null for an empty array.
fn array_shift(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let removed = mutate(arguments, |array| Ok(array.shift().unwrap_or(Value::Null)))?;
    Ok(removed.try_deref()?.clone())
}

/// Prepended values get new positions while existing string keys retain their identity.
fn array_unshift(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    mutate(arguments, |array| {
        let mut result = PhpArray::new();
        for value in arguments.iter().skip(1) {
            result.push(value.clone())?;
        }
        for (key, value) in array.iter() {
            super::slice::insert(&mut result, &key, value, super::Keys::Renumber)?;
        }
        result.reset();
        *array = result;
        Ok(Value::Int(array.len() as i64))
    })
}

/// Splice retains string keys of survivors and removed entries, but drops replacement keys.
fn array_splice(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
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
            super::slice::insert(&mut result, &key, value, super::Keys::Renumber)?;
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
                &key,
                value,
                super::Keys::Renumber,
            )?;
        }
        result.reset();
        *array = result;
        Ok(Value::from(removed))
    })
}
