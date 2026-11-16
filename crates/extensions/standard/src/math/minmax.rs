//! Minmax operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, value,
};

pub(crate) fn register(registry: &mut Registry) {
    const PARAMS: &[Param] = &[
        Param::new("value", ParamType::Mixed),
        Param::new("values", ParamType::Mixed).variadic(),
    ];
    registry.function(FunctionEntry::sync("max", PARAMS, max));
    registry.function(FunctionEntry::sync("min", PARAMS, min));
}

fn max(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    select(context, arguments, true)
}

fn min(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    select(context, arguments, false)
}

/// PHP uses native numeric comparisons until a nonnumeric value forces generic comparison.
fn select(context: &mut dyn Context, arguments: &[Value], maximum: bool) -> NativeResult {
    let name = if maximum { "max" } else { "min" };
    let values: Vec<&Value> = if arguments.len() == 1 {
        match arguments.first() {
            Some(Value::Array(array)) => array.values().collect(),
            Some(value) => {
                return Err(NativeError::type_error(format!(
                    "{name}(): Argument #1 ($value) must be of type array, {} given",
                    match value {
                        Value::Bool(false) => "false".into(),
                        Value::Bool(true) => "true".into(),
                        _ => value.type_name(),
                    }
                )));
            }
            None => Vec::new(),
        }
    } else {
        arguments.iter().collect()
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
    let mut generic = arguments.len() == 1;
    for value in iter {
        let numeric = match (best, value) {
            (Value::Int(x), Value::Int(y)) if !generic => Some(if maximum { x < y } else { x > y }),
            _ if !generic => match (exact_number(best), exact_number(value)) {
                (Some(x), Some(y)) => Some(if maximum { x < y } else { x > y }),
                _ => None,
            },
            _ => None,
        };
        let replace = if let Some(result) = numeric {
            result
        } else {
            generic = true;
            let cmp = value::compare(value, best, context.diagnostics())?;
            if maximum { cmp > 0 } else { cmp < 0 }
        };
        if replace {
            best = value;
        }
    }
    Ok(best.clone())
}

/// Avoid rounding an integer before comparison with another exact integer.
fn exact_number(value: &Value) -> Option<f64> {
    match value {
        Value::Float(f) => Some(*f),
        Value::Int(i) if (*i as f64) as i128 == i128::from(*i) => Some(*i as f64),
        _ => None,
    }
}
