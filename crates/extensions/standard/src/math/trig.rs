//! Trig operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

const UNARY_FLOAT_PARAMS: &[Param] = &[Param::new("num", ParamType::Float)];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("sin", UNARY_FLOAT_PARAMS, sin));
    registry.function(FunctionEntry::sync("cos", UNARY_FLOAT_PARAMS, cos));
    registry.function(FunctionEntry::sync("tan", UNARY_FLOAT_PARAMS, tan));
    registry.function(FunctionEntry::sync("asin", UNARY_FLOAT_PARAMS, asin));
    registry.function(FunctionEntry::sync("acos", UNARY_FLOAT_PARAMS, acos));
    registry.function(FunctionEntry::sync("atan", UNARY_FLOAT_PARAMS, atan));
    registry.function(FunctionEntry::sync("sinh", UNARY_FLOAT_PARAMS, sinh));
    registry.function(FunctionEntry::sync("cosh", UNARY_FLOAT_PARAMS, cosh));
    registry.function(FunctionEntry::sync("tanh", UNARY_FLOAT_PARAMS, tanh));
    registry.function(FunctionEntry::sync("asinh", UNARY_FLOAT_PARAMS, asinh));
    registry.function(FunctionEntry::sync("acosh", UNARY_FLOAT_PARAMS, acosh));
    registry.function(FunctionEntry::sync("atanh", UNARY_FLOAT_PARAMS, atanh));
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
