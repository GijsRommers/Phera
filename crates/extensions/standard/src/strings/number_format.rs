//! `number_format(1234567.891, 2)` is `"1,234,567.89"`.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::{buffer, check_size, decimal::FixedDigits};
use crate::math::rounding::{RoundingMode, round_to_places};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[NUMBER_FORMAT];

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
    number_format,
);

fn number_format(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let places = args::int(arguments, 1).unwrap_or(0);
    if i32::try_from(places).is_err() {
        return Err(NativeError::value_error(
            "number_format(): Argument #2 ($decimals) must be between -2147483648 and 2147483647",
        ));
    }
    let precision = places.max(0) as usize;
    let point = args::string(arguments, 2).map_or(&b"."[..], phera_extension_api::StrRef::as_bytes);
    let thousands =
        args::string(arguments, 3).map_or(&b","[..], phera_extension_api::StrRef::as_bytes);
    let number = args::float(arguments, 0).unwrap_or_default();
    // Integers must not pass through a double, which would lose the low
    // digits of numbers near PHP_INT_MAX.
    let integer = args::int(arguments, 0).or_else(|| {
        (number.abs() >= WHOLE_FLOATS_FROM
            && (i64::MIN as f64..-(i64::MIN as f64)).contains(&number))
        .then_some(number as i64)
    });
    // Preserve the string-size check before calculating or formatting digits.
    check_size(
        precision
            .saturating_add(400)
            .saturating_add(point.len())
            .saturating_add(thousands.len().saturating_mul(110)),
    )?;
    let result = if let Some(integer) = integer {
        let magnitude = rounded_magnitude(integer, places);
        let whole = integer_digits(magnitude);
        let fraction = std::iter::repeat_n(b'0', precision);
        group_thousands(
            whole,
            fraction,
            integer < 0 && magnitude != 0,
            point,
            thousands,
        )?
    } else {
        if !number.is_finite() {
            return Ok(Value::from(if number.is_nan() { "nan" } else { "inf" }));
        }
        let rounded = round_to_places(number.abs(), places, RoundingMode::HalfUp);
        let digits = FixedDigits::new(rounded, precision);
        group_thousands(
            digits.whole(),
            digits.fraction(precision),
            number < 0.0 && rounded != 0.0,
            point,
            thousands,
        )?
    };
    Ok(Value::from(result))
}

fn group_thousands(
    whole: impl ExactSizeIterator<Item = u8>,
    fraction: impl ExactSizeIterator<Item = u8>,
    negative: bool,
    point: &[u8],
    thousands: &[u8],
) -> Result<Vec<u8>, NativeError> {
    let whole_length = whole.len();
    let groups = (whole_length - 1) / 3;
    let has_fraction = fraction.len() > 0;
    let length = whole_length
        .saturating_add(groups.saturating_mul(thousands.len()))
        .saturating_add(usize::from(negative))
        .saturating_add(fraction.len());
    let mut result = buffer(if has_fraction {
        length.saturating_add(point.len())
    } else {
        length
    })?;
    if negative {
        result.push(b'-');
    }
    for (index, digit) in whole.enumerate() {
        if index > 0 && (whole_length - index).is_multiple_of(3) {
            result.extend_from_slice(thousands);
        }
        result.push(digit);
    }
    if has_fraction {
        result.extend_from_slice(point);
        result.extend(fraction);
    }
    Ok(result)
}

/// Yield an integer's decimal bytes from its highest place to its units.
fn integer_digits(mut magnitude: u64) -> impl ExactSizeIterator<Item = u8> {
    let exponent = magnitude.checked_ilog10().unwrap_or(0);
    let mut place = 10u64.pow(exponent);
    (0..exponent as usize + 1).map(move |_| {
        let digit = (magnitude / place) as u8;
        magnitude %= place;
        place /= 10;
        b'0' + digit
    })
}

/// Round an integer half up at a negative number of decimal places.
fn rounded_magnitude(integer: i64, places: i64) -> u64 {
    let magnitude = integer.unsigned_abs();
    if places < -19 {
        // 10^20 is beyond any i64.
        0
    } else if places < 0 {
        let power = 10u64.pow((-places) as u32);
        let round_up = magnitude % power >= power / 2;
        (magnitude / power + u64::from(round_up)) * power
    } else {
        magnitude
    }
}
