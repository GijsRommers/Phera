use super::*;
use phera_extension_api::value;
pub(crate) fn register(r: &mut Registry) {
    const ONE: &[Param] = &[Param::new("num", ParamType::Float)];
    for (n, f) in [
        ("is_nan", nan as SyncFn),
        ("is_finite", finite),
        ("is_infinite", infinite),
        ("deg2rad", deg),
        ("rad2deg", rad),
    ] {
        r.function(FunctionEntry::sync(n, ONE, f));
    }
    const TWO: &[Param] = &[
        Param::new("num1", ParamType::Float),
        Param::new("num2", ParamType::Float),
    ];
    for (n, f) in [("fdiv", fdiv as SyncFn), ("fmod", fmod), ("hypot", hypot)] {
        r.function(FunctionEntry::sync(n, TWO, f));
    }
    r.function(FunctionEntry::sync(
        "atan2",
        const {
            &[
                Param::new("y", ParamType::Float),
                Param::new("x", ParamType::Float),
            ]
        },
        atan2,
    ));
    r.function(FunctionEntry::sync("pi", &[], pi));
    r.function(FunctionEntry::sync(
        "abs",
        const { &[Param::new("num", ParamType::Number)] },
        abs,
    ));
    r.function(FunctionEntry::sync(
        "intdiv",
        const {
            &[
                Param::new("num1", ParamType::Int),
                Param::new("num2", ParamType::Int),
            ]
        },
        intdiv,
    ));
    r.function(FunctionEntry::sync(
        "pow",
        const {
            &[
                Param::new("num", ParamType::Mixed),
                Param::new("exponent", ParamType::Mixed),
            ]
        },
        pow,
    ));
    r.function(FunctionEntry::sync(
        "log",
        const {
            &[
                Param::new("num", ParamType::Float),
                Param::new("base", ParamType::Float).optional(),
            ]
        },
        log,
    ));
}
fn nan(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Bool(args::float(a, 0).unwrap_or_default().is_nan()))
}
fn finite(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::float(a, 0).unwrap_or_default().is_finite(),
    ))
}
fn infinite(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Bool(
        args::float(a, 0).unwrap_or_default().is_infinite(),
    ))
}
fn deg(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(a, 0).unwrap_or_default() * (std::f64::consts::PI / 180.0),
    ))
}
fn rad(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(a, 0).unwrap_or_default() * (180.0 / std::f64::consts::PI),
    ))
}
fn fdiv(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(a, 0).unwrap_or_default() / args::float(a, 1).unwrap_or_default(),
    ))
}
fn fmod(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(a, 0).unwrap_or_default() % args::float(a, 1).unwrap_or_default(),
    ))
}
fn hypot(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(a, 0)
            .unwrap_or_default()
            .hypot(args::float(a, 1).unwrap_or_default()),
    ))
}
fn atan2(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Float(
        args::float(a, 0)
            .unwrap_or_default()
            .atan2(args::float(a, 1).unwrap_or_default()),
    ))
}
fn pi(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Float(std::f64::consts::PI))
}
fn abs(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(match args::int(a, 0) {
        Some(n) if n != i64::MIN => Value::Int(n.abs()),
        _ => Value::Float(args::float(a, 0).unwrap_or_default().abs()),
    })
}
fn intdiv(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let n = args::int(a, 0).unwrap_or_default();
    let d = args::int(a, 1).unwrap_or_default();
    if d == 0 {
        return Err(NativeError::throw(
            "DivisionByZeroError",
            "Division by zero",
        ));
    }
    n.checked_div(d).map(Value::Int).ok_or_else(|| {
        NativeError::throw(
            "ArithmeticError",
            "Division of PHP_INT_MIN by -1 is not an integer",
        )
    })
}
fn pow(c: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(value::pow(
        a.first().unwrap_or(&Value::Null),
        a.get(1).unwrap_or(&Value::Null),
        c.diagnostics(),
    )?)
}
fn log(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let n = args::float(a, 0).unwrap_or_default();
    let b = args::float(a, 1).unwrap_or(std::f64::consts::E);
    if b <= 0.0 {
        return Err(NativeError::value_error(
            "log(): Argument #2 ($base) must be greater than 0",
        ));
    }
    Ok(Value::Float(if !args::given(a, 1) {
        n.ln()
    } else if b == 10.0 {
        n.log10()
    } else if b == 2.0 {
        n.log2()
    } else {
        n.ln() / b.ln()
    }))
}
