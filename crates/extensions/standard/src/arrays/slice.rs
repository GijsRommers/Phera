use super::*;
pub(crate) fn register(r: &mut Registry) {
    r.function(FunctionEntry::sync(
        "array_slice",
        const {
            &[
                Param::new("array", ParamType::Array),
                Param::new("offset", ParamType::Int),
                Param::new("length", ParamType::Int).nullable().optional(),
                Param::new("preserve_keys", ParamType::Bool).optional(),
            ]
        },
        slice,
    ));
    r.function(FunctionEntry::sync(
        "array_reverse",
        const {
            &[
                Param::new("array", ParamType::Array),
                Param::new("preserve_keys", ParamType::Bool).optional(),
            ]
        },
        reverse,
    ));
}
fn insert(out: &mut PhpArray, k: &ArrayKey, v: &Value, preserve: bool) -> Result<(), NativeError> {
    if preserve || matches!(k, ArrayKey::String(_)) {
        out.insert(k.clone(), v.clone());
    } else {
        out.push(v.clone())?;
    }
    Ok(())
}
fn slice(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mut out = PhpArray::new();
    if let Some(arr) = args::array(a, 0) {
        let offset = args::int(a, 1).unwrap_or_default();
        let start = if offset < 0 {
            arr.len().saturating_sub(offset.unsigned_abs() as usize)
        } else {
            (offset as usize).min(arr.len())
        };
        let end = match args::int(a, 2) {
            None => arr.len(),
            Some(n) if n < 0 => arr
                .len()
                .saturating_sub(n.unsigned_abs() as usize)
                .max(start),
            Some(n) => start.saturating_add(n as usize).min(arr.len()),
        };
        for (k, v) in arr.iter().skip(start).take(end - start) {
            insert(&mut out, k, v, args::bool(a, 3).unwrap_or(false))?;
        }
    }
    Ok(Value::Array(Rc::new(out)))
}
fn reverse(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mut out = PhpArray::new();
    if let Some(arr) = args::array(a, 0) {
        for (k, v) in arr.iter().collect::<Vec<_>>().into_iter().rev() {
            insert(&mut out, k, v, args::bool(a, 1).unwrap_or(false))?;
        }
    }
    Ok(Value::Array(Rc::new(out)))
}
