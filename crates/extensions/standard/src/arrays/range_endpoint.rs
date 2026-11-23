//! Endpoint coercion for php-src ext/standard/array.c: PHP_FUNCTION(range).

use phera_extension_api::value::{NumVal, parse_numeric};
use phera_extension_api::{Context, NativeError, Value};

#[derive(Clone, Copy)]
pub(super) enum NumberKind {
    Integer,
    Floating,
}

#[derive(Clone, Copy)]
pub(super) enum ByteMeaning {
    Character,
    Numeric,
}

pub(super) struct Endpoint {
    pub(super) integer: i64,
    pub(super) number: f64,
    pub(super) kind: NumberKind,
    pub(super) byte: Option<u8>,
    pub(super) byte_meaning: ByteMeaning,
}

pub(super) fn error(number: usize, name: &str, message: &str) -> NativeError {
    NativeError::value_error(format!("range(): Argument #{number} (${name}) {message}"))
}

/// Numeric single-byte strings can denote either digits or characters.
pub(super) fn endpoint(
    context: &mut dyn Context,
    value: &Value,
    number: usize,
    name: &str,
) -> Result<Endpoint, NativeError> {
    let mut result = Endpoint {
        integer: 0,
        number: 0.0,
        kind: NumberKind::Integer,
        byte: None,
        byte_meaning: ByteMeaning::Character,
    };
    match value {
        Value::Int(integer) => {
            result.integer = *integer;
            result.number = *integer as f64;
        }
        Value::Float(float) => {
            result.number = *float;
            result.kind = NumberKind::Floating;
        }
        Value::Bool(boolean) => {
            result.integer = i64::from(*boolean);
            result.number = result.integer as f64;
        }
        Value::Null => {}
        Value::String(string) => {
            parse_string(context, string.as_bytes(), number, name, &mut result);
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

/// Numeric single-byte strings remain ambiguous until both endpoints are known.
fn parse_string(
    context: &mut dyn Context,
    bytes: &[u8],
    number: usize,
    name: &str,
    result: &mut Endpoint,
) {
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
                    result.byte_meaning = ByteMeaning::Numeric;
                }
            }
            Some(NumVal::Float(float)) => {
                result.number = float;
                result.kind = NumberKind::Floating;
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
