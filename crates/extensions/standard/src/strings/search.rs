use super::*;
pub(crate) fn register(r: &mut Registry) {
    r.function(FunctionEntry::sync(
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
        r.function(FunctionEntry::sync(
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
        r.function(FunctionEntry::sync(
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
fn substr(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let s = args::bytes(a, 0);
    let offset = args::int(a, 1).unwrap_or_default();
    let start = if offset < 0 {
        s.len().saturating_sub(offset.unsigned_abs() as usize)
    } else {
        (offset as usize).min(s.len())
    };
    let end = match args::int(a, 2) {
        None => s.len(),
        Some(n) if n < 0 => s.len().saturating_sub(n.unsigned_abs() as usize).max(start),
        Some(n) => start.saturating_add(n as usize).min(s.len()),
    };
    Ok(bytes(s.get(start..end).unwrap_or_default().to_vec()))
}
fn position(a: &[Value], name: &str, fold: bool) -> NativeResult {
    let s = args::bytes(a, 0);
    let needle = args::bytes(a, 1);
    let offset = args::int(a, 2).unwrap_or_default();
    if offset.unsigned_abs() > s.len() as u64 {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
        )));
    }
    let start = if offset < 0 {
        s.len() - offset.unsigned_abs() as usize
    } else {
        offset as usize
    };
    let tail = s.get(start..).unwrap_or_default();
    let pos = if needle.is_empty() {
        Some(0)
    } else {
        tail.windows(needle.len()).position(|w| {
            if fold {
                w.eq_ignore_ascii_case(needle)
            } else {
                w == needle
            }
        })
    };
    Ok(pos.map_or(Value::Bool(false), |p| Value::Int((p + start) as i64)))
}
fn strpos(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    position(a, "strpos", false)
}
fn stripos(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    position(a, "stripos", true)
}
fn contains(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let n = args::bytes(a, 1);
    Ok(Value::Bool(
        n.is_empty() || args::bytes(a, 0).windows(n.len()).any(|w| w == n),
    ))
}
fn starts(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::bytes(a, 0).starts_with(args::bytes(a, 1)),
    ))
}
fn ends(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Bool(args::bytes(a, 0).ends_with(args::bytes(a, 1))))
}
