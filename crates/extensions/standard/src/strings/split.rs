//! Split operations for PHP values.

use phera_extension_api::value::to_php_string;
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, SyncFn, Value,
    args,
};

use super::{buffer, bytes};

const IMPLODE_PARAMS: &[Param] = &[
    Param::new("separator", ParamType::ArrayOrString),
    Param::new("array", ParamType::Array).nullable().optional(),
];

const EXPLODE_PARAMS: &[Param] = &[
    Param::new("separator", ParamType::String),
    Param::new("string", ParamType::String),
    Param::new("limit", ParamType::Int).optional(),
];

const REPEAT_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("times", ParamType::Int),
];

const SPLIT_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("length", ParamType::Int).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, function) in [("implode", implode as SyncFn), ("join", join)] {
        registry.function(FunctionEntry::sync(name, IMPLODE_PARAMS, function));
    }
    registry.function(FunctionEntry::sync("explode", EXPLODE_PARAMS, explode));
    registry.function(FunctionEntry::sync("str_repeat", REPEAT_PARAMS, repeat));
    registry.function(FunctionEntry::sync("str_split", SPLIT_PARAMS, split));
}

fn implode(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    implode_named(context, arguments, "implode")
}

fn join(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    implode_named(context, arguments, "join")
}

/// Aliases retain their own names in body-generated PHP errors.
fn implode_named(context: &mut dyn Context, arguments: &[Value], name: &str) -> NativeResult {
    let (separator, array) = if let Some(array) = args::array(arguments, 1) {
        if args::array(arguments, 0).is_some() {
            return Err(NativeError::type_error(format!(
                "{name}(): Argument #1 ($separator) must be of type string, array given"
            )));
        }
        (args::bytes(arguments, 0), array)
    } else if let Some(array) = args::array(arguments, 0) {
        (&[][..], array)
    } else {
        return Err(NativeError::type_error(format!(
            "{name}(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given"
        )));
    };
    let mut result = Vec::new();
    for (i, value) in array.values().enumerate() {
        let string = to_php_string(value, context.diagnostics())?;
        let extra = string
            .len()
            .checked_add(if i > 0 { separator.len() } else { 0 })
            .ok_or_else(|| NativeError::error("String size exceeds the maximum allowed size"))?;
        super::reserve(&mut result, extra)?;
        if i > 0 {
            result.extend_from_slice(separator);
        }
        result.extend_from_slice(string.as_bytes());
    }
    Ok(bytes(result))
}

/// Negative limits remove trailing pieces after splitting the whole input.
fn explode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let separator = args::bytes(arguments, 0);
    let string = args::bytes(arguments, 1);
    let limit = args::int(arguments, 2).unwrap_or(i64::MAX);
    if separator.is_empty() {
        return Err(NativeError::value_error(
            "explode(): Argument #1 ($separator) cannot be empty",
        ));
    }
    let mut parts = Vec::new();
    let mut start = 0;
    if limit != 0 && limit != 1 {
        while let Some(tail) = string.get(start..) {
            if limit > 0 && parts.len() >= limit as usize - 1 {
                break;
            }
            let Some(position) = super::find(tail, separator) else {
                break;
            };
            parts.push(tail.get(..position).unwrap_or_default());
            start += position + separator.len();
        }
    }
    parts.push(string.get(start..).unwrap_or_default());
    if limit < 0 {
        parts.truncate(parts.len().saturating_sub(limit.unsigned_abs() as usize));
    }
    super::list_array(parts.into_iter().map(|part| bytes(part.to_vec())))
}

/// Check the product before allocating, including for extreme repeat counts.
fn repeat(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let count = args::int(arguments, 1).unwrap_or_default();
    if count < 0 {
        return Err(NativeError::value_error(
            "str_repeat(): Argument #2 ($times) must be greater than or equal to 0",
        ));
    }
    let len = string
        .len()
        .checked_mul(count as usize)
        .ok_or_else(|| NativeError::error("String size exceeds the maximum allowed size"))?;
    let mut result = buffer(len)?;
    if !string.is_empty() {
        for _ in 0..count {
            result.extend_from_slice(string);
        }
    }
    Ok(bytes(result))
}

/// Byte chunks preserve invalid UTF-8 and embedded NULs unchanged.
fn split(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let count = args::int(arguments, 1).unwrap_or(1);
    if count < 1 {
        return Err(NativeError::value_error(
            "str_split(): Argument #2 ($length) must be greater than 0",
        ));
    }
    super::list_array(
        args::bytes(arguments, 0)
            .chunks(count as usize)
            .map(|part| bytes(part.to_vec())),
    )
}
