//! PHP 8.4+ decimal rounding, comparing boundaries in the original scale.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

const ROUND: &[Param] = &[
    Param::new("num", ParamType::Number),
    Param::new("precision", ParamType::Int).optional(),
    Param::new("mode", ParamType::Int).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("round", ROUND, round));
}

fn round(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mode = args::int(arguments, 2).unwrap_or(1);
    if !(1..=8).contains(&mode) {
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
        1 => true,
        3 => integral % 2.0 != 0.0,
        4 => integral % 2.0 == 0.0,
        _ => false,
    };
    let increment = match mode {
        5 => value > 0.0 && value.abs() > unscale(integral).abs(),
        6 => value < 0.0 && value.abs() > unscale(integral).abs(),
        7 => false,
        8 => value.abs() > unscale(integral).abs(),
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
