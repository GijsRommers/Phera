use super::*;
pub(crate) fn register(r: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("value", ParamType::Mixed),
        Param::new("values", ParamType::Mixed).variadic(),
    ];
    r.function(FunctionEntry::sync("max", PARAMS, max));
    r.function(FunctionEntry::sync("min", PARAMS, min));
}
fn max(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    select(c, a, true)
}
fn min(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    select(c, a, false)
}
fn select(c: &mut dyn Context, a: &[Value], maximum: bool) -> NativeResult {
    let name = if maximum { "max" } else { "min" };
    let values: Vec<&Value> = if a.len() == 1 {
        match a.first() {
            Some(Value::Array(arr)) => arr.values().collect(),
            Some(v) => {
                return Err(NativeError::type_error(format!(
                    "{name}(): Argument #1 ($value) must be of type array, {} given",
                    match v {
                        Value::Bool(false) => "false".into(),
                        Value::Bool(true) => "true".into(),
                        _ => v.type_name(),
                    }
                )));
            }
            None => Vec::new(),
        }
    } else {
        a.iter().collect()
    };
    let mut iter = values.into_iter();
    let Some(mut best) = iter.next() else {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #1 ($value) must contain at least one element"
        )));
    };
    // PHP's variadic numeric fast path uses native comparisons, which are
    // intentionally different from zend_compare for NaN. Once a non-numeric
    // value is seen PHP remains on the generic comparison path.
    let mut generic = a.len() == 1;
    for v in iter {
        let numeric = match (best, v) {
            (Value::Int(x), Value::Int(y)) if !generic => Some(if maximum { x < y } else { x > y }),
            _ if !generic => match (exact_number(best), exact_number(v)) {
                (Some(x), Some(y)) => Some(if maximum { x < y } else { x > y }),
                _ => None,
            },
            _ => None,
        };
        let replace = if let Some(result) = numeric {
            result
        } else {
            generic = true;
            let cmp = value::compare(v, best, c.diagnostics())?;
            if maximum { cmp > 0 } else { cmp < 0 }
        };
        if replace {
            best = v;
        }
    }
    Ok(best.clone())
}

fn exact_number(v: &Value) -> Option<f64> {
    match v {
        Value::Float(f) => Some(*f),
        Value::Int(i) if (*i as f64) as i128 == i128::from(*i) => Some(*i as f64),
        _ => None,
    }
}
