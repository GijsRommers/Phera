//! Rounding to whole numbers, roots, powers of e and logarithms, each a
//! function of one float.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value};

use super::of_float;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[CEIL, FLOOR, SQRT, EXP, LOG10, LOG1P, EXPM1];

/// `ceil(float $num): float`, the next whole number up, as a float.
///
/// ```php
/// ceil(4.1);  // 5.0
/// ceil(-4.1); // -4.0
/// ```
const CEIL: FunctionEntry =
    FunctionEntry::sync("ceil", &[Param::new("num", ParamType::Float)], ceil);

fn ceil(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::ceil))
}

/// `floor(float $num): float`, the next whole number down, as a float.
///
/// ```php
/// floor(4.9); // 4.0
/// ```
const FLOOR: FunctionEntry =
    FunctionEntry::sync("floor", &[Param::new("num", ParamType::Float)], floor);

fn floor(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::floor))
}

/// `sqrt(float $num): float`, NAN for a negative number.
const SQRT: FunctionEntry =
    FunctionEntry::sync("sqrt", &[Param::new("num", ParamType::Float)], sqrt);

fn sqrt(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::sqrt))
}

/// `exp(float $num): float`, e to the power of `$num`.
const EXP: FunctionEntry = FunctionEntry::sync("exp", &[Param::new("num", ParamType::Float)], exp);

fn exp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::exp))
}

/// `log10(float $num): float`, the base-10 logarithm.
const LOG10: FunctionEntry =
    FunctionEntry::sync("log10", &[Param::new("num", ParamType::Float)], log10);

fn log10(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::log10))
}

/// `log1p(float $num): float`, `log(1 + $num)`, exact also for `$num` near zero.
const LOG1P: FunctionEntry =
    FunctionEntry::sync("log1p", &[Param::new("num", ParamType::Float)], log1p);

fn log1p(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::ln_1p))
}

/// `expm1(float $num): float`, `exp($num) - 1`, exact also for `$num` near zero.
const EXPM1: FunctionEntry =
    FunctionEntry::sync("expm1", &[Param::new("num", ParamType::Float)], expm1);

fn expm1(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(of_float(arguments, f64::exp_m1))
}
