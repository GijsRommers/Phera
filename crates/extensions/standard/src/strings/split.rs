use super::*;
use phera_extension_api::value::to_php_string;
use std::rc::Rc;
pub(crate) fn register(r: &mut Registry) {
    for (n, f) in [("implode", implode as SyncFn), ("join", join)] {
        r.function(FunctionEntry::sync(
            n,
            const {
                &[
                    Param::new("separator", ParamType::ArrayOrString),
                    Param::new("array", ParamType::Array).nullable().optional(),
                ]
            },
            f,
        ));
    }
    r.function(FunctionEntry::sync(
        "explode",
        const {
            &[
                Param::new("separator", ParamType::String),
                Param::new("string", ParamType::String),
                Param::new("limit", ParamType::Int).optional(),
            ]
        },
        explode,
    ));
    r.function(FunctionEntry::sync(
        "str_repeat",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("times", ParamType::Int),
            ]
        },
        repeat,
    ));
    r.function(FunctionEntry::sync(
        "str_split",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("length", ParamType::Int).optional(),
            ]
        },
        split,
    ));
}
fn implode(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    implode_named(c, a, "implode")
}
fn join(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    implode_named(c, a, "join")
}
fn implode_named(c: &mut dyn Context, a: &[Value], name: &str) -> NativeResult {
    let (sep, array) = if let Some(arr) = args::array(a, 1) {
        if args::array(a, 0).is_some() {
            return Err(NativeError::type_error(format!(
                "{name}(): Argument #1 ($separator) must be of type string, array given"
            )));
        }
        (args::bytes(a, 0), arr)
    } else if let Some(arr) = args::array(a, 0) {
        (&[][..], arr)
    } else {
        return Err(NativeError::type_error(format!(
            "{name}(): If argument #1 ($separator) is of type string, argument #2 ($array) must be of type array, null given"
        )));
    };
    let mut out = Vec::new();
    for (i, v) in array.values().enumerate() {
        let s = to_php_string(v, c.diagnostics())?;
        let extra = s
            .len()
            .checked_add(if i > 0 { sep.len() } else { 0 })
            .ok_or_else(|| NativeError::error("String size exceeds the maximum allowed size"))?;
        if out.len().saturating_add(extra) > 2 * 1024 * 1024 * 1024 {
            return Err(NativeError::error(
                "String size exceeds the maximum allowed size",
            ));
        }
        out.try_reserve(extra)
            .map_err(|_| NativeError::error("Could not allocate string"))?;
        if i > 0 {
            out.extend_from_slice(sep);
        }
        out.extend_from_slice(s.as_bytes());
    }
    Ok(bytes(out))
}
fn explode(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let sep = args::bytes(a, 0);
    let s = args::bytes(a, 1);
    let limit = args::int(a, 2).unwrap_or(i64::MAX);
    if sep.is_empty() {
        return Err(NativeError::value_error(
            "explode(): Argument #1 ($separator) cannot be empty",
        ));
    }
    let mut parts = Vec::new();
    let mut start = 0;
    if limit != 0 && limit != 1 {
        while let Some(tail) = s.get(start..) {
            if limit > 0 && parts.len() >= limit as usize - 1 {
                break;
            }
            let Some(pos) = tail.windows(sep.len()).position(|w| w == sep) else {
                break;
            };
            parts.push(tail.get(..pos).unwrap_or_default());
            start += pos + sep.len();
        }
    }
    parts.push(s.get(start..).unwrap_or_default());
    if limit < 0 {
        parts.truncate(parts.len().saturating_sub(limit.unsigned_abs() as usize));
    }
    let mut arr = PhpArray::new();
    for p in parts {
        arr.push(bytes(p.to_vec()))?;
    }
    Ok(Value::Array(Rc::new(arr)))
}
fn repeat(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let s = args::bytes(a, 0);
    let n = args::int(a, 1).unwrap_or_default();
    if n < 0 {
        return Err(NativeError::value_error(
            "str_repeat(): Argument #2 ($times) must be greater than or equal to 0",
        ));
    }
    let len = s
        .len()
        .checked_mul(n as usize)
        .ok_or_else(|| NativeError::error("String size exceeds the maximum allowed size"))?;
    let mut out = buffer(len)?;
    if !s.is_empty() {
        for _ in 0..n {
            out.extend_from_slice(s);
        }
    }
    Ok(bytes(out))
}
fn split(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let n = args::int(a, 1).unwrap_or(1);
    if n < 1 {
        return Err(NativeError::value_error(
            "str_split(): Argument #2 ($length) must be greater than 0",
        ));
    }
    let mut arr = PhpArray::new();
    for part in args::bytes(a, 0).chunks(n as usize) {
        arr.push(bytes(part.to_vec()))?;
    }
    Ok(Value::Array(Rc::new(arr)))
}
