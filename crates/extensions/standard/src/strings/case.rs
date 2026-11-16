//! Case operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, SyncFn, Value, args,
};

use super::bytes;

const STRING: &[Param] = &[Param::new("string", ParamType::String)];
pub(crate) fn register(registry: &mut Registry) {
    for (n, f) in [
        ("strlen", strlen as SyncFn),
        ("strtoupper", upper),
        ("strtolower", lower),
        ("ucfirst", ucfirst),
        ("lcfirst", lcfirst),
        ("strrev", reverse),
    ] {
        registry.function(FunctionEntry::sync(n, STRING, f));
    }
    registry.function(FunctionEntry::sync(
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

fn strlen(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(args::bytes(arguments, 0).len() as i64))
}

fn upper(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(bytes(args::bytes(arguments, 0).to_ascii_uppercase()))
}

fn lower(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(bytes(args::bytes(arguments, 0).to_ascii_lowercase()))
}

fn ucfirst(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut string = args::bytes(arguments, 0).to_vec();
    if let Some(byte) = string.first_mut() {
        byte.make_ascii_uppercase();
    }
    Ok(bytes(string))
}

fn lcfirst(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut string = args::bytes(arguments, 0).to_vec();
    if let Some(byte) = string.first_mut() {
        byte.make_ascii_lowercase();
    }
    Ok(bytes(string))
}

fn reverse(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut string = args::bytes(arguments, 0).to_vec();
    string.reverse();
    Ok(bytes(string))
}

/// Only configured separator bytes start a new word; Unicode is not interpreted.
fn words(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let separator = if args::given(arguments, 1) {
        args::bytes(arguments, 1)
    } else {
        b" \t\r\n\x0c\x0b"
    };
    let mut start = true;
    let string = args::bytes(arguments, 0)
        .iter()
        .map(|byte| {
            let result = if start {
                byte.to_ascii_uppercase()
            } else {
                *byte
            };
            start = separator.contains(byte);
            result
        })
        .collect::<Vec<_>>();
    Ok(bytes(string))
}
