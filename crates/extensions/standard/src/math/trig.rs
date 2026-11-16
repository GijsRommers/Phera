use super::*;
pub(crate) fn register(r: &mut Registry) {
    const ONE: &[Param] = &[Param::new("num", ParamType::Float)];
    r.function(FunctionEntry::sync("sin", ONE, sin));
    r.function(FunctionEntry::sync("cos", ONE, cos));
    r.function(FunctionEntry::sync("tan", ONE, tan));
    r.function(FunctionEntry::sync("asin", ONE, asin));
    r.function(FunctionEntry::sync("acos", ONE, acos));
    r.function(FunctionEntry::sync("atan", ONE, atan));
    r.function(FunctionEntry::sync("sinh", ONE, sinh));
    r.function(FunctionEntry::sync("cosh", ONE, cosh));
    r.function(FunctionEntry::sync("tanh", ONE, tanh));
    r.function(FunctionEntry::sync("asinh", ONE, asinh));
    r.function(FunctionEntry::sync("acosh", ONE, acosh));
    r.function(FunctionEntry::sync("atanh", ONE, atanh));
}
fn sin(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().sin()))
}
fn cos(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().cos()))
}
fn tan(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().tan()))
}
fn asin(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().asin()))
}
fn acos(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().acos()))
}
fn atan(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().atan()))
}
fn sinh(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().sinh()))
}
fn cosh(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().cosh()))
}
fn tanh(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().tanh()))
}
fn asinh(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().asinh()))
}
fn acosh(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().acosh()))
}
fn atanh(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().atanh()))
}
