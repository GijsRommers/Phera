use super::*;
use phera_extension_api::value::{loose_equals, strict_equals};
pub(crate) fn register(r: &mut Registry) {
    const ONE: &[Param] = &[Param::new("array", ParamType::Array)];
    for (n, f) in [
        ("array_values", values as SyncFn),
        ("array_key_first", first_key),
        ("array_key_last", last_key),
        ("array_first", first),
        ("array_last", last),
        ("array_is_list", is_list),
    ] {
        r.function(FunctionEntry::sync(n, ONE, f));
    }
    r.function(FunctionEntry::sync(
        "array_keys",
        const {
            &[
                Param::new("array", ParamType::Array),
                Param::new("filter_value", ParamType::Mixed).optional(),
                Param::new("strict", ParamType::Bool).optional(),
            ]
        },
        keys,
    ));
    for (n, f) in [("in_array", contains as SyncFn), ("array_search", search)] {
        r.function(FunctionEntry::sync(
            n,
            const {
                &[
                    Param::new("needle", ParamType::Mixed),
                    Param::new("haystack", ParamType::Array),
                    Param::new("strict", ParamType::Bool).optional(),
                ]
            },
            f,
        ));
    }
    for n in ["array_key_exists", "key_exists"] {
        r.function(FunctionEntry::sync(
            n,
            const {
                &[
                    Param::new("key", ParamType::Mixed),
                    Param::new("array", ParamType::Array),
                ]
            },
            exists,
        ));
    }
}
fn values(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mut out = PhpArray::new();
    if let Some(arr) = args::array(a, 0) {
        for v in arr.values() {
            out.push(v.clone())?;
        }
    }
    Ok(Value::Array(Rc::new(out)))
}
fn first_key(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(args::array(a, 0)
        .and_then(|a| a.first())
        .map_or(Value::Null, |(k, _)| key_value(k)))
}
fn last_key(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(args::array(a, 0)
        .and_then(|a| a.last())
        .map_or(Value::Null, |(k, _)| key_value(k)))
}
fn first(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(args::array(a, 0)
        .and_then(|a| a.first())
        .map_or(Value::Null, |(_, v)| v.clone()))
}
fn last(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(args::array(a, 0)
        .and_then(|a| a.last())
        .map_or(Value::Null, |(_, v)| v.clone()))
}
fn is_list(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Bool(args::array(a, 0).is_none_or(|a| a.is_list())))
}
fn equal(c: &mut dyn Context, a: &Value, b: &Value, strict: bool) -> Result<bool, NativeError> {
    Ok(matches!(
        if strict {
            strict_equals(a, b, c.diagnostics())?
        } else {
            loose_equals(a, b, c.diagnostics())?
        },
        Value::Bool(true)
    ))
}
fn keys(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mut out = PhpArray::new();
    if let Some(arr) = args::array(a, 0) {
        for (k, v) in arr.iter() {
            if !args::given(a, 1)
                || equal(
                    c,
                    v,
                    a.get(1).unwrap_or(&Value::Null),
                    args::bool(a, 2).unwrap_or(false),
                )?
            {
                out.push(key_value(k))?;
            }
        }
    }
    Ok(Value::Array(Rc::new(out)))
}
fn find(c: &mut dyn Context, a: &[Value]) -> Result<Option<Value>, NativeError> {
    if let Some(arr) = args::array(a, 1) {
        for (k, v) in arr.iter() {
            if equal(
                c,
                a.first().unwrap_or(&Value::Null),
                v,
                args::bool(a, 2).unwrap_or(false),
            )? {
                return Ok(Some(key_value(k)));
            }
        }
    }
    Ok(None)
}
fn search(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(find(c, a)?.unwrap_or(Value::Bool(false)))
}
fn contains(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Bool(find(c, a)?.is_some()))
}
fn exists(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    let v = a.first().unwrap_or(&Value::Null);
    let k = if v.is_null() {
        c.deprecated("Using null as the key parameter for array_key_exists() is deprecated, use an empty string instead".into());
        ArrayKey::String(PhpString::new())
    } else {
        ArrayKey::from_value_diag(v, c.diagnostics())?
    };
    Ok(Value::Bool(
        args::array(a, 1).is_some_and(|arr| arr.contains_key(&k)),
    ))
}
