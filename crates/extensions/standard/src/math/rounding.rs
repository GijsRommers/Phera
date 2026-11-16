//! Port of php-src ext/standard/math.c: _php_math_round and php_round_helper.
//! PHP 8.4+ compares rounding boundaries in the original scale.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

#[derive(Clone, Copy)]
enum RoundingMode {
    HalfUp,
    HalfDown,
    HalfEven,
    HalfOdd,
    Ceiling,
    Floor,
    TowardZero,
    AwayFromZero,
}

impl RoundingMode {
    fn from_integer(mode: i64) -> Option<Self> {
        match mode {
            1 => Some(Self::HalfUp),
            2 => Some(Self::HalfDown),
            3 => Some(Self::HalfEven),
            4 => Some(Self::HalfOdd),
            5 => Some(Self::Ceiling),
            6 => Some(Self::Floor),
            7 => Some(Self::TowardZero),
            8 => Some(Self::AwayFromZero),
            _ => None,
        }
    }
}

const ROUND_PARAMS: &[Param] = &[
    Param::new("num", ParamType::Number),
    Param::new("precision", ParamType::Int).optional(),
    Param::new("mode", ParamType::Int).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("round", ROUND_PARAMS, round));
}

fn round(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mode = args::int(arguments, 2).unwrap_or(1);
    if RoundingMode::from_integer(mode).is_none() {
        return Err(NativeError::value_error(
            "round(): Argument #3 ($mode) must be a valid rounding mode (RoundingMode::*)",
        ));
    }
    Ok(Value::Float(decimal_round(
        args::float(arguments, 0).unwrap_or_default(),
        args::int(arguments, 1).unwrap_or_default(),
        mode,
    )))
}

/// Testing the half boundary after rescaling avoids PHP's former pre-rounding
/// heuristic and preserves values such as 0.285 at decimal boundaries.
pub(crate) fn decimal_round(value: f64, places: i64, mode: i64) -> f64 {
    round_decimal(
        value,
        places,
        RoundingMode::from_integer(mode).unwrap_or(RoundingMode::HalfDown),
    )
}

/// Shared callers retain the numeric mode interface; the algorithm uses named modes.
fn round_decimal(value: f64, places: i64, mode: RoundingMode) -> f64 {
    if !value.is_finite() || value == 0.0 || (places == 0 && value == value.trunc()) {
        return value;
    }
    let places = places.clamp(i64::from(i32::MIN) + 1, i64::from(i32::MAX)) as i32;
    let exponent = 10f64.powf(f64::from(places.abs()));
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
