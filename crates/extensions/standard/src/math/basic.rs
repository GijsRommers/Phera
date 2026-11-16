//! Basic operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

pub(crate) fn register(registry: &mut Registry) {
    const ONE: &[Param] = &[Param::new("num", ParamType::Float)];
    registry.function(FunctionEntry::sync("ceil", ONE, ceil));
    registry.function(FunctionEntry::sync("floor", ONE, floor));
    registry.function(FunctionEntry::sync("sqrt", ONE, sqrt));
    registry.function(FunctionEntry::sync("exp", ONE, exp));
    registry.function(FunctionEntry::sync("log10", ONE, log10));
    registry.function(FunctionEntry::sync("log1p", ONE, log1p));
    registry.function(FunctionEntry::sync("expm1", ONE, expm1));
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
