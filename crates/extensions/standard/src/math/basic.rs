//! Basic operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

const UNARY_FLOAT_PARAMS: &[Param] = &[Param::new("num", ParamType::Float)];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("ceil", UNARY_FLOAT_PARAMS, ceil));
    registry.function(FunctionEntry::sync("floor", UNARY_FLOAT_PARAMS, floor));
    registry.function(FunctionEntry::sync("sqrt", UNARY_FLOAT_PARAMS, sqrt));
    registry.function(FunctionEntry::sync("exp", UNARY_FLOAT_PARAMS, exp));
    registry.function(FunctionEntry::sync("log10", UNARY_FLOAT_PARAMS, log10));
    registry.function(FunctionEntry::sync("log1p", UNARY_FLOAT_PARAMS, log1p));
    registry.function(FunctionEntry::sync("expm1", UNARY_FLOAT_PARAMS, expm1));
}

fn ceil(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().ceil(),
    ))
}

fn floor(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().floor(),
    ))
}

fn sqrt(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().sqrt(),
    ))
}

fn exp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().exp(),
    ))
}

fn log10(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().log10(),
    ))
}

fn log1p(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().ln_1p(),
    ))
}

fn expm1(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().exp_m1(),
    ))
}
