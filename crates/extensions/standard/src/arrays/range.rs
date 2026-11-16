//! PHP 8.3+ numeric and byte ranges, including ambiguous single-digit strings.
use phera_extension_api::value::{NumVal, parse_numeric};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};
const PARAMETERS: &[Param] = &[
    Param::new("start", ParamType::Mixed),
    Param::new("end", ParamType::Mixed),
    Param::new("step", ParamType::Mixed).optional(),
];
const PREPARED_PARAMETERS: &[Param] = &[
    Param::new("start", ParamType::Mixed),
    Param::new("end", ParamType::Mixed),
    Param::new("step", ParamType::Number).optional(),
];
pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("range", PARAMETERS, range));
}
struct Endpoint {
    integer: i64,
    number: f64,
    float: bool,
    byte: Option<u8>,
    ambiguous: bool,
}
fn error(number: usize, name: &str, message: &str) -> NativeError {
    NativeError::value_error(format!("range(): Argument #{number} (${name}) {message}"))
}
/// Numeric single-byte strings can denote either digits or characters.
fn endpoint(
    context: &mut dyn Context,
    value: &Value,
    number: usize,
    name: &str,
) -> Result<Endpoint, NativeError> {
    let mut result = Endpoint {
        integer: 0,
        number: 0.0,
        float: false,
        byte: None,
        ambiguous: false,
    };
    match value {
        Value::Int(integer) => {
            result.integer = *integer;
            result.number = *integer as f64;
        }
        Value::Float(float) => {
            result.number = *float;
            result.float = true;
        }
        Value::Bool(boolean) => {
            result.integer = i64::from(*boolean);
            result.number = result.integer as f64;
        }
        Value::Null => {}
        Value::String(string) => {
            let bytes = string.as_bytes();
            if bytes.is_empty() {
                context.warning(format!(
                    "range(): Argument #{number} (${name}) must not be empty, casted to 0"
                ));
            } else {
                let parsed = parse_numeric(bytes);
                match parsed.value.filter(|_| parsed.exact) {
                    Some(NumVal::Int(integer)) => {
                        result.integer = integer;
                        result.number = integer as f64;
                        if bytes.len() == 1 {
                            result.byte = bytes.first().copied();
                            result.ambiguous = true;
                        }
                    }
                    Some(NumVal::Float(float)) => {
                        result.number = float;
                        result.float = true;
                    }
                    None => {
                        if bytes.len() != 1 {
                            context.warning(format!("range(): Argument #{number} (${name}) must be a single byte, subsequent bytes are ignored"));
                        }
                        result.byte = bytes.first().copied();
                    }
                }
            }
        }
        _ => {
            return Err(NativeError::type_error(format!(
                "range(): Argument #{number} (${name}) must be of type string|int|float, {} given",
                value.type_name()
            )));
        }
    }
    if !result.number.is_finite() {
        return Err(error(
            number,
            name,
            if result.number.is_nan() {
                "must be a finite number, NAN provided"
            } else {
                "must be a finite number, INF provided"
            },
        ));
    }
    Ok(result)
}
/// The endpoint union cannot be declared in ParamType. Parse it before the
/// step so null deprecations and type errors retain PHP's argument order.
fn range(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    for (index, name) in [(0, "start"), (1, "end")] {
        let value = args::value(arguments, index);
        let scalar = matches!(value, Value::Int(_) | Value::Float(_) | Value::String(_));
        let coercible = matches!(value, Value::Null | Value::Bool(_));
        if !scalar && (context.strict_types() || !coercible) {
            let given = match value {
                Value::Bool(true) => "true".into(),
                Value::Bool(false) => "false".into(),
                _ => value.type_name(),
            };
            return Err(NativeError::type_error(format!(
                "range(): Argument #{} (${name}) must be of type string|int|float, {given} given",
                index + 1
            )));
        }
        if value.is_null() {
            context.deprecated(format!("range(): Passing null to parameter #{} (${name}) of type string|int|float is deprecated",index+1));
        }
    }
    let mut prepared = arguments.to_vec();
    phera_extension_api::prepare_arguments(
        &FunctionEntry::sync("range", PREPARED_PARAMETERS, range_values),
        &mut prepared,
        context,
    )?;
    range_values(context, &prepared)
}
/// A negative step is valid only for a decreasing or constant range.
fn range_values(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let signed_step = args::float(arguments, 2).unwrap_or(1.0);
    if !signed_step.is_finite() {
        return Err(error(
            3,
            "step",
            if signed_step.is_nan() {
                "must be a finite number, NAN provided"
            } else {
                "must be a finite number, INF provided"
            },
        ));
    }
    if args::int(arguments, 2) == Some(i64::MIN) {
        return Err(error(
            3,
            "step",
            "must be greater than -9223372036854775808",
        ));
    }
    if signed_step == 0.0 {
        return Err(error(3, "step", "cannot be 0"));
    }
    let step = signed_step.abs();
    let fractional = args::int(arguments, 2).is_none()
        && (step.fract() != 0.0 || step >= 9_223_372_036_854_775_808.0);
    let integer_step = args::int(arguments, 2)
        .map(i64::unsigned_abs)
        .unwrap_or(step as u64);
    let mut start = endpoint(context, args::value(arguments, 0), 1, "start")?;
    let mut end = endpoint(context, args::value(arguments, 1), 2, "end")?;
    let characters = start.byte.is_some() && end.byte.is_some() && !fractional;
    if start.byte.is_some() != end.byte.is_some() {
        let (number, name, other_number, other_name, ambiguous) = if start.byte.is_some() {
            (2, "end", 1, "start", start.ambiguous)
        } else {
            (1, "start", 2, "end", end.ambiguous)
        };
        if !ambiguous {
            context.warning(format!("range(): Argument #{number} (${name}) must be a single byte string if argument #{other_number} (${other_name}) is a single byte string, argument #{other_number} (${other_name}) converted to 0"));
        }
    } else if start.byte.is_some() && fractional && (!start.ambiguous || !end.ambiguous) {
        context.warning("range(): Argument #3 ($step) must be of type int when generating an array of characters, inputs converted to 0".into());
    }
    if characters {
        start.integer = i64::from(start.byte.unwrap_or_default());
        end.integer = i64::from(end.byte.unwrap_or_default());
        start.number = start.integer as f64;
        end.number = end.integer as f64;
    }
    let floating = start.float || end.float || fractional;
    let increasing = if floating {
        start.number < end.number
    } else {
        start.integer < end.integer
    };
    let equal = if floating {
        start.number == end.number
    } else {
        start.integer == end.integer
    };
    if increasing && signed_step < 0.0 {
        return Err(error(
            3,
            "step",
            "must be greater than 0 for increasing ranges",
        ));
    }
    let distance = if floating {
        (end.number - start.number).abs()
    } else {
        (i128::from(end.integer) - i128::from(start.integer)).unsigned_abs() as f64
    };
    if !equal && distance < step {
        return Err(error(
            3,
            "step",
            "must be less than the range spanned by argument #1 ($start) and argument #2 ($end)",
        ));
    }
    let count = if equal {
        1.0
    } else {
        if floating {
            (distance / step + 1.0).round()
        } else {
            ((i128::from(end.integer) - i128::from(start.integer)).unsigned_abs()
                / u128::from(integer_step)
                + 1) as f64
        }
    };
    if count >= 1_073_741_824.0 {
        let low = start.number.min(end.number);
        let high = start.number.max(end.number);
        let message = if floating {
            format!(
                "The supplied range exceeds the maximum array size by {:.1} elements: start={low:.1}, end={high:.1}, step={step:.1}. Max size: 1073741824",
                distance / step + 1.0 - 1_073_741_824.0
            )
        } else {
            let calculated = (i128::from(end.integer) - i128::from(start.integer)).unsigned_abs()
                / u128::from(integer_step);
            format!(
                "The supplied range exceeds the maximum array size by {} elements: start={}, end={}, step={}. Calculated size: {calculated}. Maximum size: 1073741824.",
                calculated.saturating_sub(1_073_741_823),
                start.integer.min(end.integer),
                start.integer.max(end.integer),
                integer_step
            )
        };
        return Err(NativeError::value_error(message));
    }
    let mut result = PhpArray::new();
    for index in 0..count as u64 {
        let value = if floating {
            Value::Float(if index == 0 {
                start.number
            } else if increasing {
                start.number + index as f64 * step
            } else {
                start.number - index as f64 * step
            })
        } else {
            let delta = i128::from(index) * i128::from(integer_step);
            let integer = if increasing {
                i128::from(start.integer) + delta
            } else {
                i128::from(start.integer) - delta
            };
            if characters {
                Value::from(vec![integer as u8])
            } else {
                Value::Int(integer as i64)
            }
        };
        if let Value::Float(number) = &value
            && ((increasing && *number > end.number) || (!increasing && *number < end.number))
        {
            break;
        }
        result.push(value)?;
    }
    Ok(Value::from(result))
}
