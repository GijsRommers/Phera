//! Search operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, SyncFn, Value,
    args,
};

use super::bytes;

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "substr",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("offset", ParamType::Int),
                Param::new("length", ParamType::Int).nullable().optional(),
            ]
        },
        substr,
    ));
    for (n, f) in [("strpos", strpos as SyncFn), ("stripos", stripos)] {
        registry.function(FunctionEntry::sync(
            n,
            const {
                &[
                    Param::new("haystack", ParamType::String),
                    Param::new("needle", ParamType::String),
                    Param::new("offset", ParamType::Int).optional(),
                ]
            },
            f,
        ));
    }
    for (n, f) in [
        ("str_contains", contains as SyncFn),
        ("str_starts_with", starts),
        ("str_ends_with", ends),
    ] {
        registry.function(FunctionEntry::sync(
            n,
            const {
                &[
                    Param::new("haystack", ParamType::String),
                    Param::new("needle", ParamType::String),
                ]
            },
            f,
        ));
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
        Some(n) if n < 0 => string
            .len()
            .saturating_sub(n.unsigned_abs() as usize)
            .max(start),
        Some(n) => start.saturating_add(n as usize).min(string.len()),
    };
    Ok(bytes(string.get(start..end).unwrap_or_default().to_vec()))
}

/// Search offsets outside the haystack throw rather than silently clamp.
fn position(arguments: &[Value], name: &str, fold: bool) -> NativeResult {
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
    let pos = if fold {
        super::find_ignore_case(tail, needle)
    } else {
        super::find(tail, needle)
    };
    Ok(pos.map_or(Value::Bool(false), |p| Value::Int((p + start) as i64)))
}

fn strpos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    position(arguments, "strpos", false)
}

fn stripos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    position(arguments, "stripos", true)
}

fn contains(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let n = args::bytes(arguments, 1);
    Ok(Value::Bool(
        super::find(args::bytes(arguments, 0), n).is_some(),
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
