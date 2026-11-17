//! Case operations for PHP values.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, SyncFn, Value, args,
};

use super::bytes;

const STRING_PARAMS: &[Param] = &[Param::new("string", ParamType::String)];
const UCWORDS_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("separators", ParamType::String).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, function) in [
        ("strlen", strlen as SyncFn),
        ("strtoupper", uppercase),
        ("strtolower", lowercase),
        ("ucfirst", ucfirst),
        ("lcfirst", lcfirst),
        ("strrev", reverse),
    ] {
        registry.function(FunctionEntry::sync(name, STRING_PARAMS, function));
    }
    registry.function(FunctionEntry::sync(
        "ucwords",
        UCWORDS_PARAMS,
        capitalize_words,
    ));
}

fn strlen(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(args::bytes(arguments, 0).len() as i64))
}

fn uppercase(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(bytes(args::bytes(arguments, 0).to_ascii_uppercase()))
}

fn lowercase(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(bytes(args::bytes(arguments, 0).to_ascii_lowercase()))
}

/// PHP changes only the first byte, without Unicode case expansion.
fn ucfirst(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut string = args::bytes(arguments, 0).to_vec();
    if let Some(byte) = string.first_mut() {
        byte.make_ascii_uppercase();
    }
    Ok(bytes(string))
}

/// PHP changes only the first byte, without Unicode case expansion.
fn lcfirst(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut string = args::bytes(arguments, 0).to_vec();
    if let Some(byte) = string.first_mut() {
        byte.make_ascii_lowercase();
    }
    Ok(bytes(string))
}

/// PHP reverses bytes, including individual bytes of multibyte characters.
fn reverse(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut string = args::bytes(arguments, 0).to_vec();
    string.reverse();
    Ok(bytes(string))
}

/// Only configured separator bytes word_start a new word; Unicode is not interpreted.
fn capitalize_words(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let separator = if args::given(arguments, 1) {
        args::bytes(arguments, 1)
    } else {
        b" \t\r\n\x0c\x0b"
    };
    let mut word_start = true;
    let string = args::bytes(arguments, 0)
        .iter()
        .map(|byte| {
            let result = if word_start {
                byte.to_ascii_uppercase()
            } else {
                *byte
            };
            word_start = separator.contains(byte);
            result
        })
        .collect::<Vec<_>>();
    Ok(bytes(string))
}
