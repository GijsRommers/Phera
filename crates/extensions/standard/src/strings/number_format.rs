//! `number_format(1234567.891, 2)` is `"1,234,567.89"`.

use phera_extension_api::{
    Change, Context, FunctionEntry, LanguageLevel, NativeError, NativeFn, NativeResult, Param,
    ParamType, PhpString, Value, args,
};

use super::{buffer, decimal};
use crate::math::rounding::{RoundingMode, round_to_places};

pub(super) fn entry(level: LanguageLevel) -> FunctionEntry {
    if level.has(Change::NumberFormatDecimals) {
        FunctionEntry {
            handler: NativeFn::Sync(number_format::<true>),
            ..NUMBER_FORMAT
        }
    } else {
        NUMBER_FORMAT
    }
}

/// From 2^52 on, every double is a whole number, so it is formatted as an
/// integer when it also fits in one.
const WHOLE_FLOATS_FROM: f64 = 4_503_599_627_370_496.0;

/// `number_format(float $num, int $decimals = 0, ?string $decimal_separator
/// = ".", ?string $thousands_separator = ","): string`
///
/// The number is rounded half up to `$decimals` places (tens, hundreds…
/// when negative) and grouped by thousands; a result that rounds to zero
/// has no minus sign.
///
/// ```php
/// number_format(1234.5);                     // "1,235"
/// number_format(1234.5678, 2, ",", ".");     // "1.234,57"
/// number_format(-0.4);                       // "0"
/// number_format(PHP_INT_MAX);                // "9,223,372,036,854,775,807", exact
/// ```
const NUMBER_FORMAT: FunctionEntry = FunctionEntry::sync(
    "number_format",
    &[
        Param::new("num", ParamType::Number),
        Param::new("decimals", ParamType::Int).optional(),
        Param::new("decimal_separator", ParamType::String)
            .nullable()
            .optional(),
        Param::new("thousands_separator", ParamType::String)
            .nullable()
            .optional(),
    ],
    number_format::<false>,
);

fn number_format<const VALIDATE_DECIMALS: bool>(
    _: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    let places = args::int(arguments, 1).unwrap_or(0);
    if VALIDATE_DECIMALS && i32::try_from(places).is_err() {
        return Err(NativeError::value_error(
            "number_format(): Argument #2 ($decimals) must be between -2147483648 and 2147483647",
        ));
    }
    let places = places.clamp(i64::from(i32::MIN), i64::from(i32::MAX));
    let precision = places.max(0) as usize;
    let point = args::string(arguments, 2).map_or(&b"."[..], PhpString::as_bytes);
    let thousands = args::string(arguments, 3).map_or(&b","[..], PhpString::as_bytes);
    let number = args::float(arguments, 0).unwrap_or_default();
    // Integers must not pass through a double, which would lose the low
    // digits of numbers near PHP_INT_MAX.
    let integer = args::int(arguments, 0).or_else(|| {
        (number.abs() >= WHOLE_FLOATS_FROM
            && (i64::MIN as f64..-(i64::MIN as f64)).contains(&number))
        .then_some(number as i64)
    });
    // Room for the longest result, reserved before any digit is made, so
    // that a huge `$decimals` is PHP's size error: up to 400 digits, a
    // thousands separator for each three of them, and the point.
    let mut result = buffer(
        precision
            .saturating_add(400)
            .saturating_add(point.len())
            .saturating_add(thousands.len().saturating_mul(110)),
    )?;
    let (digits, negative) = if let Some(integer) = integer {
        format_integer(integer, places, precision)
    } else {
        if !number.is_finite() {
            return Ok(Value::from(if number.is_nan() { "nan" } else { "inf" }));
        }
        let rounded = round_to_places(number.abs(), places, RoundingMode::HalfUp);
        (
            decimal::fixed(rounded, precision),
            number < 0.0 && rounded != 0.0,
        )
    };
    if negative {
        result.push(b'-');
    }
    group_thousands(&mut result, &digits, precision, point, thousands);
    Ok(Value::from(result))
}

/// `digits`, a plain `1234.5`, with the separators put in.
fn group_thousands(
    result: &mut Vec<u8>,
    digits: &[u8],
    precision: usize,
    point: &[u8],
    thousands: &[u8],
) {
    let whole = digits
        .iter()
        .position(|&byte| byte == b'.')
        .unwrap_or(digits.len());
    for (index, &digit) in digits[..whole].iter().enumerate() {
        if index > 0 && (whole - index).is_multiple_of(3) {
            result.extend_from_slice(thousands);
        }
        result.push(digit);
    }
    if precision > 0 {
        result.extend_from_slice(point);
        result.extend_from_slice(&digits[whole + 1..]);
    }
}

/// An integer's digits, rounded half up to `places` when that is negative,
/// with `precision` zeros after the point, and whether it needs a minus.
fn format_integer(integer: i64, places: i64, precision: usize) -> (Vec<u8>, bool) {
    let mut magnitude = integer.unsigned_abs();
    if places < -19 {
        // 10^20 is beyond any i64.
        magnitude = 0;
    } else if places < 0 {
        let power = 10u64.pow((-places) as u32);
        let round_up = magnitude % power >= power / 2;
        magnitude = (magnitude / power + u64::from(round_up)) * power;
    }
    let mut digits = magnitude.to_string().into_bytes();
    if precision > 0 {
        digits.push(b'.');
        digits.resize(digits.len() + precision, b'0');
    }
    (digits, integer < 0 && magnitude != 0)
}
