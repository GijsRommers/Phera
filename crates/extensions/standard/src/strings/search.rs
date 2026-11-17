//! Search operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, SyncFn, Value,
    args,
};

use super::{CaseSensitivity, bytes};

const SUBSTR_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("offset", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
];

const POSITION_PARAMS: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    Param::new("offset", ParamType::Int).optional(),
];

const PREDICATE_PARAMS: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("substr", SUBSTR_PARAMS, substr));
    for (name, function) in [("strpos", strpos as SyncFn), ("stripos", stripos)] {
        registry.function(FunctionEntry::sync(name, POSITION_PARAMS, function));
    }
    for (name, function) in [
        ("str_contains", contains as SyncFn),
        ("str_starts_with", starts),
        ("str_ends_with", ends),
    ] {
        registry.function(FunctionEntry::sync(name, PREDICATE_PARAMS, function));
    }
}

/// Negative lengths exclude bytes from the end and never reverse the slice.
fn substr(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let offset = args::int(arguments, 1).unwrap_or_default();
    let start = if offset < 0 {
        string.len().saturating_sub(offset.unsigned_abs() as usize)
    } else {
        (offset as usize).min(string.len())
    };
    let end = match args::int(arguments, 2) {
        None => string.len(),
        Some(length) if length < 0 => string
            .len()
            .saturating_sub(length.unsigned_abs() as usize)
            .max(start),
        Some(length) => start.saturating_add(length as usize).min(string.len()),
    };
    Ok(bytes(string.get(start..end).unwrap_or_default().to_vec()))
}

/// Search offsets outside the haystack throw rather than silently clamp.
fn position(arguments: &[Value], name: &str, sensitivity: CaseSensitivity) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let needle = args::bytes(arguments, 1);
    let offset = args::int(arguments, 2).unwrap_or_default();
    if offset.unsigned_abs() > string.len() as u64 {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
        )));
    }
    let start = if offset < 0 {
        string.len() - offset.unsigned_abs() as usize
    } else {
        offset as usize
    };
    let tail = string.get(start..).unwrap_or_default();
    let found = if sensitivity == CaseSensitivity::Insensitive {
        super::find_ignore_case(tail, needle)
    } else {
        super::find(tail, needle)
    };
    Ok(found.map_or(Value::Bool(false), |position| {
        Value::Int((position + start) as i64)
    }))
}

fn strpos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    position(arguments, "strpos", CaseSensitivity::Sensitive)
}

fn stripos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    position(arguments, "stripos", CaseSensitivity::Insensitive)
}

fn contains(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let length = args::bytes(arguments, 1);
    Ok(Value::Bool(
        super::find(args::bytes(arguments, 0), length).is_some(),
    ))
}

fn starts(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::bytes(arguments, 0).starts_with(args::bytes(arguments, 1)),
    ))
}

fn ends(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::bytes(arguments, 0).ends_with(args::bytes(arguments, 1)),
    ))
}
