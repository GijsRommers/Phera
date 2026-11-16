//! Decimal grouping with exact integer rounding and PHP's float rounding.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::{buffer, bytes, decimal};
use crate::math::rounding::decimal_round;

const PARAMETERS: &[Param] = &[
    Param::new("num", ParamType::Number),
    Param::new("decimals", ParamType::Int).optional(),
    Param::new("decimal_separator", ParamType::String)
        .nullable()
        .optional(),
    Param::new("thousands_separator", ParamType::String)
        .nullable()
        .optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "number_format",
        PARAMETERS,
        number_format,
    ));
}

/// Integer inputs must not pass through f64: doing so loses low digits near
/// PHP_INT_MAX, even when the caller requests no fractional places.
fn number_format(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let places = args::int(arguments, 1).unwrap_or(0);
    if places < i64::from(i32::MIN) || places > i64::from(i32::MAX) {
        return Err(NativeError::value_error(
            "number_format(): Argument #2 ($decimals) must be between -2147483648 and 2147483647",
        ));
    }
    let precision = places.max(0) as usize;
    let separator = args::string(arguments, 2).map_or(b".".as_slice(), |string| string.as_bytes());
    let thousands = args::string(arguments, 3).map_or(b",".as_slice(), |string| string.as_bytes());
    let value = args::float(arguments, 0).unwrap_or_default();
    let integer = args::int(arguments, 0).or_else(|| {
        (value.abs() >= 4503599627370496.0
            && (-9223372036854775808.0..9223372036854775808.0).contains(&value))
        .then_some(value as i64)
    });
    let mut result = buffer(
        precision
            .saturating_add(400)
            .saturating_add(separator.len())
            .saturating_add(thousands.len().saturating_mul(110)),
    )?;
    let (text, negative) = if let Some(integer) = integer {
        let mut magnitude = integer.unsigned_abs();
        if places < -19 {
            magnitude = 0;
        } else if places < 0 {
            let power = 10u64.pow((-places) as u32);
            magnitude = (magnitude / power + u64::from(magnitude % power >= power / 2)) * power;
        }
        let mut text = magnitude.to_string().into_bytes();
        if precision > 0 {
            text.push(b'.');
            text.resize(text.len() + precision, b'0');
        }
        (text, integer < 0 && magnitude != 0)
    } else {
        if !value.is_finite() {
            return Ok(bytes(if value.is_nan() {
                b"nan".to_vec()
            } else {
                b"inf".to_vec()
            }));
        }
        let rounded = decimal_round(value.abs(), places, 1);
        (
            decimal::fixed(rounded, precision),
            value < 0.0 && rounded != 0.0,
        )
    };
    if negative {
        result.push(b'-');
    }
    let point = text
        .iter()
        .position(|byte| *byte == b'.')
        .unwrap_or(text.len());
    for (index, byte) in text.iter().take(point).enumerate() {
        if index > 0 && (point - index).is_multiple_of(3) {
            result.extend_from_slice(thousands);
        }
        result.push(*byte);
    }
    if precision > 0 {
        result.extend_from_slice(separator);
        result.extend_from_slice(text.get(point + 1..).unwrap_or_default());
    }
    Ok(bytes(result))
}
