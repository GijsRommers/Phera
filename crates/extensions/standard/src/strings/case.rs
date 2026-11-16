use super::*;
const STRING: &[Param] = &[Param::new("string", ParamType::String)];
pub(crate) fn register(r: &mut Registry) {
    for (n, f) in [
        ("strlen", strlen as SyncFn),
        ("strtoupper", upper),
        ("strtolower", lower),
        ("ucfirst", ucfirst),
        ("lcfirst", lcfirst),
        ("strrev", reverse),
    ] {
        r.function(FunctionEntry::sync(n, STRING, f));
    }
    r.function(FunctionEntry::sync(
        "ucwords",
        const {
            &[
                Param::new("string", ParamType::String),
                Param::new("separators", ParamType::String).optional(),
            ]
        },
        words,
    ));
}
fn strlen(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(Value::Int(args::bytes(a, 0).len() as i64))
}
fn upper(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(bytes(args::bytes(a, 0).to_ascii_uppercase()))
}
fn lower(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    Ok(bytes(args::bytes(a, 0).to_ascii_lowercase()))
}
fn ucfirst(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mut s = args::bytes(a, 0).to_vec();
    if let Some(c) = s.first_mut() {
        c.make_ascii_uppercase();
    }
    Ok(bytes(s))
}
fn lcfirst(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mut s = args::bytes(a, 0).to_vec();
    if let Some(c) = s.first_mut() {
        c.make_ascii_lowercase();
    }
    Ok(bytes(s))
}
fn reverse(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let mut s = args::bytes(a, 0).to_vec();
    s.reverse();
    Ok(bytes(s))
}
fn words(_: &mut dyn Context, a: &[Value]) -> NativeResult {
    let sep = if args::given(a, 1) {
        args::bytes(a, 1)
    } else {
        b" \t\r\n\x0c\x0b"
    };
    let mut start = true;
    let s = args::bytes(a, 0)
        .iter()
        .map(|c| {
            let out = if start { c.to_ascii_uppercase() } else { *c };
            start = sep.contains(c);
            out
        })
        .collect::<Vec<_>>();
    Ok(bytes(s))
}
