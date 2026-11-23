//! `round(2.5)` is 3.0, `round(1.955, 2)` is 1.96: ported from php-src
//! ext/standard/math.c, `_php_math_round` and `php_round_helper`.
//!
//! Since PHP 8.4 the value is compared with the rounding boundary in its
//! original scale, so 0.285 rounds to 0.29 although its double is a little
//! below 0.285 times 100.

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, ConstantValue, Context, FunctionEntry, NativeError,
    NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::constants::{
    PHP_ROUND_HALF_DOWN, PHP_ROUND_HALF_EVEN, PHP_ROUND_HALF_ODD, PHP_ROUND_HALF_UP,
};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[ROUND];

/// PHP exposes these modes through enum cases and accepts the historical integers.
#[derive(Clone, Copy)]
pub(crate) enum RoundingMode {
    HalfUp,
    HalfDown,
    HalfEven,
    HalfOdd,
    Ceiling,
    Floor,
    TowardZero,
    AwayFromZero,
}

const CASES: &[&str] = &[
    "HalfAwayFromZero",
    "HalfTowardsZero",
    "HalfEven",
    "HalfOdd",
    "TowardsZero",
    "AwayFromZero",
    "NegativeInfinity",
    "PositiveInfinity",
];

pub(super) fn register_enum(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::UnitEnum(CASES),
        ..ClassEntry::class("RoundingMode")
    });
}

impl RoundingMode {
    fn from_case(value: &Value) -> Option<Self> {
        let Value::Object(object) = value else {
            return Self::from_integer(args::int(std::slice::from_ref(value), 0)?);
        };
        let name = object.get(&ArrayKey::str("name"))?;
        let name = name.as_str_bytes()?;
        Some(match name {
            b"HalfAwayFromZero" => Self::HalfUp,
            b"HalfTowardsZero" => Self::HalfDown,
            b"HalfEven" => Self::HalfEven,
            b"HalfOdd" => Self::HalfOdd,
            b"TowardsZero" => Self::TowardZero,
            b"AwayFromZero" => Self::AwayFromZero,
            b"NegativeInfinity" => Self::Floor,
            b"PositiveInfinity" => Self::Ceiling,
            _ => return None,
        })
    }

    fn from_integer(mode: i64) -> Option<Self> {
        Some(match mode {
            PHP_ROUND_HALF_UP => Self::HalfUp,
            PHP_ROUND_HALF_DOWN => Self::HalfDown,
            PHP_ROUND_HALF_EVEN => Self::HalfEven,
            PHP_ROUND_HALF_ODD => Self::HalfOdd,
            5 => Self::Ceiling,
            6 => Self::Floor,
            7 => Self::TowardZero,
            8 => Self::AwayFromZero,
            _ => return None,
        })
    }
}

/// `round(int|float $num, int $precision = 0, int|RoundingMode $mode =
/// RoundingMode::HalfAwayFromZero): float`, rounded to `$precision` decimal places
/// (tens, hundreds… when negative).
///
/// ```php
/// round(2.5);                         // 3.0
/// round(2.5, 0, PHP_ROUND_HALF_EVEN); // 2.0
/// round(1234.5678, -2);               // 1200.0
/// ```
const ROUND: FunctionEntry = FunctionEntry::sync(
    "round",
    &[
        Param::new("num", ParamType::Number),
        Param::new("precision", ParamType::Int).with_default(ConstantValue::Int(0)),
        Param::new("mode", ParamType::RoundingModeOrInt).optional(),
    ],
    round,
);

fn round(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mode = match arguments.get(2) {
        Some(value) => RoundingMode::from_case(value),
        None => Some(RoundingMode::HalfUp),
    };
    let Some(mode) = mode else {
        return Err(NativeError::value_error(
            "round(): Argument #3 ($mode) must be a valid rounding mode (RoundingMode::*)",
        ));
    };
    let number = args::float(arguments, 0).unwrap_or_default();
    let places = args::int(arguments, 1).unwrap_or_default();
    if places >= 0 && matches!(arguments.first(), Some(Value::Int(_))) {
        return Ok(Value::Float(number));
    }
    Ok(Value::Float(round_to_places(number, places, mode)))
}

/// `value` rounded to `places` decimal places, as `_php_math_round` does.
pub(crate) fn round_to_places(value: f64, places: i64, mode: RoundingMode) -> f64 {
    if !value.is_finite() || value == 0.0 || (places == 0 && value == value.trunc()) {
        return value;
    }
    let places = places.clamp(i64::from(i32::MIN) + 1, i64::from(i32::MAX)) as i32;
    let exponent = decimal_power(places.unsigned_abs());
    let scaled = if places > 0 {
        value * exponent
    } else {
        value / exponent
    };
    let mut integral = scaled.trunc();
    let adjacent = integral + 1.0f64.copysign(value);
    let unscale = |number: f64| {
        if places > 0 {
            number / exponent
        } else {
            number * exponent
        }
    };
    if unscale(adjacent) == value {
        integral = adjacent;
    }
    if integral.abs() >= 1e16 {
        return value;
    }
    let edge = unscale(integral + 0.5f64.copysign(integral)).abs();
    let tie_up = match mode {
        RoundingMode::HalfUp => true,
        RoundingMode::HalfEven => integral % 2.0 != 0.0,
        RoundingMode::HalfOdd => integral % 2.0 == 0.0,
        _ => false,
    };
    let increment = match mode {
        RoundingMode::Ceiling => value > 0.0 && value.abs() > unscale(integral).abs(),
        RoundingMode::Floor => value < 0.0 && value.abs() > unscale(integral).abs(),
        RoundingMode::TowardZero => false,
        RoundingMode::AwayFromZero => value.abs() > unscale(integral).abs(),
        _ => value.abs() > edge || (value.abs() == edge && tie_up),
    };
    if increment {
        integral += 1.0f64.copysign(integral);
    }
    if places.abs() < 23 {
        unscale(integral)
    } else {
        // PHP snprintf drops the negative sign of zero on this fallback path.
        let integral = if integral == 0.0 { 0.0 } else { integral };
        let result = format!("{integral:.6}e{}", -places)
            .parse::<f64>()
            .unwrap_or(value);
        if result.is_finite() { result } else { value }
    }
}

/// PHP uses exact float literals for the first 23 decimal powers.
fn decimal_power(power: u32) -> f64 {
    const POWERS: [f64; 23] = [
        1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
        1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
    ];
    POWERS
        .get(power as usize)
        .copied()
        .unwrap_or_else(|| 10f64.powf(f64::from(power)))
}
