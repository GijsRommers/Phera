//! Trig operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

pub(crate) fn register(registry: &mut Registry) {
    const ONE: &[Param] = &[Param::new("num", ParamType::Float)];
    registry.function(FunctionEntry::sync("sin", ONE, sin));
    registry.function(FunctionEntry::sync("cos", ONE, cos));
    registry.function(FunctionEntry::sync("tan", ONE, tan));
    registry.function(FunctionEntry::sync("asin", ONE, asin));
    registry.function(FunctionEntry::sync("acos", ONE, acos));
    registry.function(FunctionEntry::sync("atan", ONE, atan));
    registry.function(FunctionEntry::sync("sinh", ONE, sinh));
    registry.function(FunctionEntry::sync("cosh", ONE, cosh));
    registry.function(FunctionEntry::sync("tanh", ONE, tanh));
    registry.function(FunctionEntry::sync("asinh", ONE, asinh));
    registry.function(FunctionEntry::sync("acosh", ONE, acosh));
    registry.function(FunctionEntry::sync("atanh", ONE, atanh));
}

fn sin(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().sin(),
    ))
}

fn cos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().cos(),
    ))
}

fn tan(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().tan(),
    ))
}

fn asin(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().asin(),
    ))
}

fn acos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().acos(),
    ))
}

fn atan(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().atan(),
    ))
}

fn sinh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().sinh(),
    ))
}

fn cosh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().cosh(),
    ))
}

fn tanh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().tanh(),
    ))
}

fn asinh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().asinh(),
    ))
}

fn acosh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().acosh(),
    ))
}

fn atanh(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(arguments, 0).unwrap_or_default().atanh(),
    ))
}
