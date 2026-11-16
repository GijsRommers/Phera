use super::*;
pub(crate) fn register(r: &mut Registry) {
    const ONE: &[Param] = &[Param::new("num", ParamType::Float)];
    r.function(FunctionEntry::sync("ceil", ONE, ceil));
    r.function(FunctionEntry::sync("floor", ONE, floor));
    r.function(FunctionEntry::sync("sqrt", ONE, sqrt));
    r.function(FunctionEntry::sync("exp", ONE, exp));
    r.function(FunctionEntry::sync("log10", ONE, log10));
    r.function(FunctionEntry::sync("log1p", ONE, log1p));
    r.function(FunctionEntry::sync("expm1", ONE, expm1));
}
fn ceil(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().ceil()))
}
fn floor(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().floor()))
}
fn sqrt(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().sqrt()))
}
fn exp(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().exp()))
}
fn log10(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().log10()))
}
fn log1p(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().ln_1p()))
}
fn expm1(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(args::float(a, 0).unwrap_or_default().exp_m1()))
}
