//! PHP's byte-oriented formatted output and positional argument parser.

use phera_extension_api::value::{float_repr, to_float, to_int, to_php_string};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::{buffer, bytes, decimal};

const VARIADIC: &[Param] = &[
    Param::new("format", ParamType::String),
    Param::new("values", ParamType::Mixed).variadic(),
];
const ARRAY: &[Param] = &[
    Param::new("format", ParamType::String),
    Param::new("values", ParamType::Array),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("sprintf", VARIADIC, sprintf));
    registry.function(FunctionEntry::sync("printf", VARIADIC, printf));
    registry.function(FunctionEntry::sync("vsprintf", ARRAY, vsprintf));
    registry.function(FunctionEntry::sync("vprintf", ARRAY, vprintf));
}

fn sprintf(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, "sprintf", false, false)
}

fn printf(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, "printf", false, true)
}

fn vsprintf(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, "vsprintf", true, false)
}

fn vprintf(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(context, arguments, "vprintf", true, true)
}

/// The vector forms consume values in insertion order, ignoring array keys.
fn run(
    context: &mut dyn Context,
    arguments: &[Value],
    name: &str,
    vector: bool,
    output: bool,
) -> NativeResult {
    let values: Vec<Value> = if vector {
        args::array(arguments, 1)
            .map(|array| array.values().cloned().collect())
            .unwrap_or_default()
    } else {
        arguments.get(1..).unwrap_or_default().to_vec()
    };
    let result = render(context, args::bytes(arguments, 0), &values, name, vector)?;
    if output {
        context.write(&result);
        Ok(Value::Int(result.len() as i64))
    } else {
        Ok(bytes(result))
    }
}

/// Decimal fields are bounded before allocation or narrowing to a machine int.
fn number(format: &[u8], cursor: &mut usize, label: &str) -> Result<usize, NativeError> {
    let mut result = 0usize;
    while let Some(byte) = format.get(*cursor).filter(|byte| byte.is_ascii_digit()) {
        result = result
            .saturating_mul(10)
            .saturating_add(usize::from(*byte - b'0'));
        *cursor += 1;
    }
    if result > i32::MAX as usize {
        return Err(NativeError::value_error(format!(
            "{label} must be between 0 and 2147483647"
        )));
    }
    Ok(result)
}

/// A positional argument does not advance the sequential argument cursor.
fn position(format: &[u8], cursor: &mut usize) -> Result<Option<usize>, NativeError> {
    let start = *cursor;
    let mut end = start;
    while format.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == start || format.get(end) != Some(&b'$') {
        return Ok(None);
    }
    let value = number(format, cursor, "Argument number specifier");
    *cursor = end + 1;
    match value {
        Ok(value) if value > 0 => Ok(Some(value - 1)),
        _ => Err(NativeError::value_error(
            "Argument number specifier must be greater than zero and less than 2147483647",
        )),
    }
}

fn missing(_name: &str, vector: bool, needed: usize, given: usize) -> NativeError {
    if vector {
        NativeError::value_error(format!(
            "The arguments array must contain {needed} items, {given} given"
        ))
    } else {
        NativeError::argument_count_error(format!(
            "{needed} arguments are required, {given} given",
            needed = needed + 1,
            given = given + 1
        ))
    }
}

/// PHP 8.5 accepts star fields, but requires an actual integer for them.
fn star(
    format: &[u8],
    cursor: &mut usize,
    next: &mut usize,
    values: &[Value],
    label: &str,
    needed: &mut usize,
) -> Result<Option<i64>, NativeError> {
    *cursor += 1;
    let index = position(format, cursor)?.unwrap_or_else(|| {
        let index = *next;
        *next += 1;
        index
    });
    let Some(value) = values.get(index) else {
        *needed = (*needed).max(index + 1);
        return Ok(None);
    };
    let Value::Int(value) = &*value.deref() else {
        return Err(NativeError::value_error(format!(
            "{label} must be an integer"
        )));
    };
    let minimum = if label == "Precision" { -1 } else { 0 };
    if *value < minimum || *value > i64::from(i32::MAX) {
        return Err(NativeError::value_error(format!(
            "{label} must be between {minimum} and 2147483647"
        )));
    }
    Ok(Some(*value))
}

/// Rendering is buffered so printf emits nothing if a later specifier fails.
fn render(
    context: &mut dyn Context,
    format: &[u8],
    values: &[Value],
    name: &str,
    vector: bool,
) -> Result<Vec<u8>, NativeError> {
    let mut result = buffer(format.len())?;
    let mut cursor = 0;
    let mut next = 0;
    let mut needed = 0usize;
    while let Some(&byte) = format.get(cursor) {
        cursor += 1;
        if byte != b'%' {
            result.push(byte);
            continue;
        }
        if format.get(cursor) == Some(&b'%') {
            result.push(b'%');
            cursor += 1;
            continue;
        }
        let selected = position(format, &mut cursor)?;
        let mut padding = b' ';
        let mut left = false;
        let mut sign = false;
        loop {
            match format.get(cursor) {
                Some(b' ' | b'0') => padding = format.get(cursor).copied().unwrap_or(b' '),
                Some(b'-') => left = true,
                Some(b'+') => sign = true,
                Some(b'\'') => {
                    cursor += 1;
                    padding = *format
                        .get(cursor)
                        .ok_or_else(|| NativeError::value_error("Missing padding character"))?;
                }
                _ => break,
            }
            cursor += 1;
        }
        let width = if format.get(cursor) == Some(&b'*') {
            match star(format, &mut cursor, &mut next, values, "Width", &mut needed)? {
                Some(width) => width as usize,
                None => continue,
            }
        } else {
            number(format, &mut cursor, "Width")?
        };
        let mut precision = None;
        let mut explicit = false;
        if format.get(cursor) == Some(&b'.') {
            cursor += 1;
            explicit = format
                .get(cursor)
                .is_some_and(|byte| byte.is_ascii_digit() || *byte == b'*');
            precision = Some(if format.get(cursor) == Some(&b'*') {
                match star(
                    format,
                    &mut cursor,
                    &mut next,
                    values,
                    "Precision",
                    &mut needed,
                )? {
                    Some(precision) => precision,
                    None => continue,
                }
            } else {
                number(format, &mut cursor, "Precision")? as i64
            });
        }
        if format.get(cursor) == Some(&b'l') {
            cursor += 1;
        }
        let index = selected.unwrap_or_else(|| {
            let index = next;
            next += 1;
            index
        });
        let Some(value) = values.get(index) else {
            needed = needed.max(index + 1);
            continue;
        };
        let conversion = *format
            .get(cursor)
            .ok_or_else(|| NativeError::value_error("Missing format specifier at end of string"))?;
        cursor += 1;
        if precision == Some(-1) && !matches!(conversion, b'g' | b'G' | b'h' | b'H') {
            return Err(NativeError::value_error(
                "Precision -1 is only supported for %g, %G, %h and %H",
            ));
        }
        let mut text = match conversion {
            b's' => {
                let mut text = to_php_string(value, context.diagnostics())?
                    .as_bytes()
                    .to_vec();
                if explicit {
                    text.truncate(precision.unwrap_or(0) as usize);
                }
                text
            }
            b'd' => {
                let number = to_int(value, context.diagnostics())?;
                if sign && number >= 0 {
                    format!("+{number}").into_bytes()
                } else {
                    number.to_string().into_bytes()
                }
            }
            b'u' => (to_int(value, context.diagnostics())? as u64)
                .to_string()
                .into_bytes(),
            b'b' => format!("{:b}", to_int(value, context.diagnostics())? as u64).into_bytes(),
            b'o' => format!("{:o}", to_int(value, context.diagnostics())? as u64).into_bytes(),
            b'x' => format!("{:x}", to_int(value, context.diagnostics())? as u64).into_bytes(),
            b'X' => format!("{:X}", to_int(value, context.diagnostics())? as u64).into_bytes(),
            b'c' => {
                result.push(to_int(value, context.diagnostics())? as u8);
                continue;
            }
            b'%' => {
                result.push(b'%');
                continue;
            }
            b'e' | b'E' | b'f' | b'F' | b'g' | b'G' | b'h' | b'H' => {
                let number = to_float(value, context.diagnostics())?;
                let mut precision = precision.unwrap_or(6);
                if precision > 53 {
                    context.notice(format!("{name}(): Requested precision of {precision} digits was truncated to PHP maximum of 53 digits"));
                    precision = 53;
                }
                if !number.is_finite() {
                    result.extend_from_slice(if number.is_nan() {
                        b"NaN"
                    } else if number < 0.0 {
                        b"-INF"
                    } else {
                        b"INF"
                    });
                    continue;
                }
                let mut text = if precision == -1 {
                    float_repr(number.abs()).into_bytes()
                } else if matches!(conversion, b'f' | b'F') {
                    decimal::fixed(number, precision as usize)
                } else {
                    decimal::significant(number, precision as usize, conversion)
                };
                if number < 0.0
                    || (number == 0.0
                        && number.is_sign_negative()
                        && matches!(conversion, b'g' | b'G' | b'h' | b'H'))
                {
                    text.insert(0, b'-');
                } else if sign {
                    text.insert(0, b'+');
                }
                text
            }
            0 => return Err(NativeError::value_error("Unknown format specifier \"")),
            _ => {
                return Err(NativeError::value_error(format!(
                    "Unknown format specifier \"{}\"",
                    char::from(conversion)
                )));
            }
        };
        if explicit && matches!(conversion, b'b' | b'o' | b'x' | b'X') {
            text.clear();
        }
        if left && padding == b'0' && matches!(conversion, b'd' | b'u') {
            padding = b' ';
        }
        let count = width.saturating_sub(text.len());
        super::reserve(&mut result, text.len().saturating_add(count))?;
        if !left
            && padding == b'0'
            && matches!(
                conversion,
                b'd' | b'e' | b'E' | b'f' | b'F' | b'g' | b'G' | b'h' | b'H'
            )
            && text.first().is_some_and(|byte| matches!(byte, b'-' | b'+'))
        {
            result.push(text.remove(0));
        }
        if !left {
            result.resize(result.len() + count, padding);
        }
        result.extend_from_slice(&text);
        if left {
            result.resize(result.len() + count, padding);
        }
    }
    if needed > 0 {
        return Err(missing(name, vector, needed, values.len()));
    }
    Ok(result)
}
