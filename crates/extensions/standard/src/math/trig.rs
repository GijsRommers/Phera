//! Trigonometric and hyperbolic functions, in radians: `sin(M_PI_2)` is 1.0.
//! PHP calls the C library, and so does Rust's `f64`.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value};

use super::of_float;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    SIN, COS, TAN, ASIN, ACOS, ATAN, SINH, COSH, TANH, ASINH, ACOSH, ATANH,
];

/// `sin(float $num): float`
const SIN: FunctionEntry = FunctionEntry::sync("sin", &[Param::new("num", ParamType::Float)], sin);

fn sin(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::sin))
}

/// `cos(float $num): float`
const COS: FunctionEntry = FunctionEntry::sync("cos", &[Param::new("num", ParamType::Float)], cos);

fn cos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::cos))
}

/// `tan(float $num): float`
const TAN: FunctionEntry = FunctionEntry::sync("tan", &[Param::new("num", ParamType::Float)], tan);

fn tan(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::tan))
}

/// `asin(float $num): float`
const ASIN: FunctionEntry =
    FunctionEntry::sync("asin", &[Param::new("num", ParamType::Float)], asin);

fn asin(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::asin))
}

/// `acos(float $num): float`
const ACOS: FunctionEntry =
    FunctionEntry::sync("acos", &[Param::new("num", ParamType::Float)], acos);

fn acos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::acos))
}

/// `atan(float $num): float`
const ATAN: FunctionEntry =
    FunctionEntry::sync("atan", &[Param::new("num", ParamType::Float)], atan);

fn atan(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::atan))
}

/// `sinh(float $num): float`
const SINH: FunctionEntry =
    FunctionEntry::sync("sinh", &[Param::new("num", ParamType::Float)], sinh);

fn sinh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::sinh))
}

/// `cosh(float $num): float`
const COSH: FunctionEntry =
    FunctionEntry::sync("cosh", &[Param::new("num", ParamType::Float)], cosh);

fn cosh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::cosh))
}

/// `tanh(float $num): float`
const TANH: FunctionEntry =
    FunctionEntry::sync("tanh", &[Param::new("num", ParamType::Float)], tanh);

fn tanh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::tanh))
}

/// `asinh(float $num): float`
const ASINH: FunctionEntry =
    FunctionEntry::sync("asinh", &[Param::new("num", ParamType::Float)], asinh);

fn asinh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::asinh))
}

/// `acosh(float $num): float`
const ACOSH: FunctionEntry =
    FunctionEntry::sync("acosh", &[Param::new("num", ParamType::Float)], acosh);

fn acosh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::acosh))
}

/// `atanh(float $num): float`
const ATANH: FunctionEntry =
    FunctionEntry::sync("atanh", &[Param::new("num", ParamType::Float)], atanh);

fn atanh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::atanh))
}
