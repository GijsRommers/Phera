//! The other number functions: `abs`, `intdiv`, `pow`, `log`, `fmod`,
//! `fdiv`, `hypot`, `atan2`, `pi`, the angle conversions and the float
//! tests.

use std::f64::consts::{E, PI};

use phera_extension_api::value;
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::of_float;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    IS_NAN,
    IS_FINITE,
    IS_INFINITE,
    DEG2RAD,
    RAD2DEG,
    FDIV,
    FMOD,
    HYPOT,
    ATAN2,
    PI_FUNCTION,
    ABS,
    INTDIV,
    POW,
    LOG,
];

/// The two float arguments of `fdiv`, `fmod`, `hypot` and `atan2`.
fn two_floats(arguments: &[Value]) -> (f64, f64) {
    (
        args::float(arguments, 0).unwrap_or_default(),
        args::float(arguments, 1).unwrap_or_default(),
    )
}

/// `is_nan(float $num): bool`
const IS_NAN: FunctionEntry =
    FunctionEntry::sync("is_nan", &[Param::new("num", ParamType::Float)], is_nan);

fn is_nan(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::float(arguments, 0).unwrap_or_default().is_nan(),
    ))
}

/// `is_finite(float $num): bool`, false for INF, -INF and NAN.
const IS_FINITE: FunctionEntry = FunctionEntry::sync(
    "is_finite",
    &[Param::new("num", ParamType::Float)],
    is_finite,
);

fn is_finite(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::float(arguments, 0).unwrap_or_default().is_finite(),
    ))
}

/// `is_infinite(float $num): bool`, true for INF and -INF.
const IS_INFINITE: FunctionEntry = FunctionEntry::sync(
    "is_infinite",
    &[Param::new("num", ParamType::Float)],
    is_infinite,
);

fn is_infinite(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::float(arguments, 0).unwrap_or_default().is_infinite(),
    ))
}

/// `deg2rad(float $num): float`
///
/// ```php
/// deg2rad(180); // M_PI
/// ```
const DEG2RAD: FunctionEntry =
    FunctionEntry::sync("deg2rad", &[Param::new("num", ParamType::Float)], deg2rad);

fn deg2rad(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    // PHP multiplies by the rounded constant, which `to_radians` does not.
    Ok(of_float(arguments, |degrees| degrees * (PI / 180.0)))
}

/// `rad2deg(float $num): float`
const RAD2DEG: FunctionEntry =
    FunctionEntry::sync("rad2deg", &[Param::new("num", ParamType::Float)], rad2deg);

fn rad2deg(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, |radians| radians * (180.0 / PI)))
}

/// `fdiv(float $num1, float $num2): float`, IEEE division: dividing by zero
/// is INF, -INF or NAN instead of an error.
///
/// ```php
/// fdiv(1, 0); // INF
/// ```
const FDIV: FunctionEntry = FunctionEntry::sync(
    "fdiv",
    &[
        Param::new("num1", ParamType::Float),
        Param::new("num2", ParamType::Float),
    ],
    fdiv,
);

fn fdiv(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (dividend, divisor) = two_floats(arguments);
    Ok(Value::Float(dividend / divisor))
}

/// `fmod(float $num1, float $num2): float`, the remainder with the sign of
/// `$num1`, as C's `fmod`.
///
/// ```php
/// fmod(-7.5, 2); // -1.5
/// ```
const FMOD: FunctionEntry = FunctionEntry::sync(
    "fmod",
    &[
        Param::new("num1", ParamType::Float),
        Param::new("num2", ParamType::Float),
    ],
    fmod,
);

fn fmod(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (dividend, divisor) = two_floats(arguments);
    Ok(Value::Float(dividend % divisor))
}

/// `hypot(float $num1, float $num2): float`, the length of the hypotenuse,
/// without overflow on the way.
const HYPOT: FunctionEntry = FunctionEntry::sync(
    "hypot",
    &[
        Param::new("num1", ParamType::Float),
        Param::new("num2", ParamType::Float),
    ],
    hypot,
);

fn hypot(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (x, y) = two_floats(arguments);
    Ok(Value::Float(x.hypot(y)))
}

/// `atan2(float $y, float $x): float`, the angle of the point (x, y).
const ATAN2: FunctionEntry = FunctionEntry::sync(
    "atan2",
    &[
        Param::new("y", ParamType::Float),
        Param::new("x", ParamType::Float),
    ],
    atan2,
);

fn atan2(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (y, x) = two_floats(arguments);
    Ok(Value::Float(y.atan2(x)))
}

/// `pi(): float`, the same as `M_PI`.
const PI_FUNCTION: FunctionEntry = FunctionEntry::sync("pi", &[], pi);

fn pi(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Float(PI))
}

/// `abs(int|float $num): int|float`
///
/// `PHP_INT_MIN` has no positive int, so its absolute value is a float.
///
/// ```php
/// abs(-5);          // 5
/// abs(PHP_INT_MIN); // 9.2233720368547758E+18
/// ```
const ABS: FunctionEntry = FunctionEntry::sync("abs", &[Param::new("num", ParamType::Number)], abs);

fn abs(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(match args::int(arguments, 0) {
        Some(number) if number != i64::MIN => Value::Int(number.abs()),
        _ => Value::Float(args::float(arguments, 0).unwrap_or_default().abs()),
    })
}

/// `intdiv(int $num1, int $num2): int`, division rounded towards zero.
///
/// ```php
/// intdiv(-7, 2);           // -3
/// intdiv(1, 0);            // DivisionByZeroError
/// intdiv(PHP_INT_MIN, -1); // ArithmeticError
/// ```
const INTDIV: FunctionEntry = FunctionEntry::sync(
    "intdiv",
    &[
        Param::new("num1", ParamType::Int),
        Param::new("num2", ParamType::Int),
    ],
    intdiv,
);

fn intdiv(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let number = args::int(arguments, 0).unwrap_or_default();
    let divisor = args::int(arguments, 1).unwrap_or_default();
    if divisor == 0 {
        return Err(NativeError::throw(
            "DivisionByZeroError",
            "Division by zero",
        ));
    }
    // The only other division that does not fit is PHP_INT_MIN / -1.
    number.checked_div(divisor).map(Value::Int).ok_or_else(|| {
        NativeError::throw(
            "ArithmeticError",
            "Division of PHP_INT_MIN by -1 is not an integer",
        )
    })
}

/// `pow(mixed $num, mixed $exponent): int|float|object`, the `**` operator.
///
/// ```php
/// pow(2, 10);  // 1024
/// pow(2, -1);  // 0.5
/// ```
const POW: FunctionEntry = FunctionEntry::sync(
    "pow",
    &[
        Param::new("num", ParamType::Mixed),
        Param::new("exponent", ParamType::Mixed),
    ],
    pow,
);

fn pow(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(value::pow(
        &arguments[0],
        &arguments[1],
        context.diagnostics(),
    )?)
}

/// `log(float $num, float $base = M_E): float`
///
/// Like PHP, bases 2 and 10 use their own C functions, which are exact
/// where `log($num) / log($base)` is not.
///
/// ```php
/// log(M_E);     // 1.0
/// log(1000, 10); // 3.0
/// ```
const LOG: FunctionEntry = FunctionEntry::sync(
    "log",
    &[
        Param::new("num", ParamType::Float),
        Param::new("base", ParamType::Float).optional(),
    ],
    log,
);

fn log(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let number = args::float(arguments, 0).unwrap_or_default();
    let base = args::float(arguments, 1).unwrap_or(E);
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
