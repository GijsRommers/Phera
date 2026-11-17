//! Misc operations for PHP values.

use phera_extension_api::value;
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, SyncFn, Value,
    args,
};

const UNARY_FLOAT_PARAMS: &[Param] = &[Param::new("num", ParamType::Float)];

const BINARY_FLOAT_PARAMS: &[Param] = &[
    Param::new("num1", ParamType::Float),
    Param::new("num2", ParamType::Float),
];

const ATAN2_PARAMS: &[Param] = &[
    Param::new("y", ParamType::Float),
    Param::new("x", ParamType::Float),
];

const ABS_PARAMS: &[Param] = &[Param::new("num", ParamType::Number)];

const INTDIV_PARAMS: &[Param] = &[
    Param::new("num1", ParamType::Int),
    Param::new("num2", ParamType::Int),
];

const POW_PARAMS: &[Param] = &[
    Param::new("num", ParamType::Mixed),
    Param::new("exponent", ParamType::Mixed),
];

const LOG_PARAMS: &[Param] = &[
    Param::new("num", ParamType::Float),
    Param::new("base", ParamType::Float).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, function) in [
        ("is_nan", is_nan as SyncFn),
        ("is_finite", is_finite),
        ("is_infinite", is_infinite),
        ("deg2rad", deg2rad),
        ("rad2deg", rad2deg),
    ] {
        registry.function(FunctionEntry::sync(name, UNARY_FLOAT_PARAMS, function));
    }

    for (name, function) in [("fdiv", fdiv as SyncFn), ("fmod", fmod), ("hypot", hypot)] {
        registry.function(FunctionEntry::sync(name, BINARY_FLOAT_PARAMS, function));
    }
    registry.function(FunctionEntry::sync("atan2", ATAN2_PARAMS, atan2));
    registry.function(FunctionEntry::sync("pi", &[], pi));
    registry.function(FunctionEntry::sync("abs", ABS_PARAMS, abs));
    registry.function(FunctionEntry::sync("intdiv", INTDIV_PARAMS, intdiv));
    registry.function(FunctionEntry::sync("pow", POW_PARAMS, pow));
    registry.function(FunctionEntry::sync("log", LOG_PARAMS, log));
}

fn is_nan(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::float(arguments, 0).unwrap_or_default().is_nan(),
    ))
}

fn is_finite(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::float(arguments, 0).unwrap_or_default().is_finite(),
    ))
}

fn is_infinite(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::float(arguments, 0).unwrap_or_default().is_infinite(),
    ))
}

fn deg2rad(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default() * (std::f64::consts::PI / 180.0),
    ))
}

fn rad2deg(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default() * (180.0 / std::f64::consts::PI),
    ))
}

fn fdiv(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default()
            / args::float(arguments, 1).unwrap_or_default(),
    ))
}

fn fmod(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default()
            % args::float(arguments, 1).unwrap_or_default(),
    ))
}

fn hypot(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0)
            .unwrap_or_default()
            .hypot(args::float(arguments, 1).unwrap_or_default()),
    ))
}

fn atan2(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0)
            .unwrap_or_default()
            .atan2(args::float(arguments, 1).unwrap_or_default()),
    ))
}

fn pi(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Float(std::f64::consts::PI))
}

/// The signed minimum has no positive integer counterpart, so PHP returns a float.
fn abs(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(match args::int(arguments, 0) {
        Some(number) if number != i64::MIN => Value::Int(number.abs()),
        _ => Value::Float(args::float(arguments, 0).unwrap_or_default().abs()),
    })
}

/// PHP distinguishes zero divisors from the signed minimum divided by minus one.
fn intdiv(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let number = args::int(arguments, 0).unwrap_or_default();
    let divisor = args::int(arguments, 1).unwrap_or_default();
    if divisor == 0 {
        return Err(NativeError::throw(
            "DivisionByZeroError",
            "Division by zero",
        ));
    }
    number.checked_div(divisor).map(Value::Int).ok_or_else(|| {
        NativeError::throw(
            "ArithmeticError",
            "Division of PHP_INT_MIN by -1 is not an integer",
        )
    })
}

fn pow(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(value::pow(
        arguments.first().unwrap_or(&Value::Null),
        arguments.get(1).unwrap_or(&Value::Null),
        context.diagnostics(),
    )?)
}

/// Omitted bases use ln directly; explicit bases retain PHP's specialized paths.
fn log(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let number = args::float(arguments, 0).unwrap_or_default();
    let base = args::float(arguments, 1).unwrap_or(std::f64::consts::E);
    if base <= 0.0 {
        return Err(NativeError::value_error(
            "log(): Argument #2 ($base) must be greater than 0",
        ));
    }
    Ok(Value::Float(if !args::given(arguments, 1) {
        number.ln()
    } else if base == 10.0 {
        number.log10()
    } else if base == 2.0 {
        number.log2()
    } else {
        number.ln() / base.ln()
    }))
}
