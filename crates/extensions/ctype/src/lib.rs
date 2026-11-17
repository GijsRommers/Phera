//! ASCII character classes matching PHP with LC_CTYPE=C on Linux servers.

use phera_extension_api::{
    Context, Extension, FunctionEntry, NativeResult, Param, ParamType, Registry, SyncFn, Value,
};

pub struct Ctype;

const CTYPE_PARAMS: &[Param] = &[Param::new("text", ParamType::Mixed)];

impl Extension for Ctype {
    fn name(&self) -> &'static str {
        "ctype"
    }

    fn register(&self, registry: &mut Registry) {
        for (name, function) in FUNCTIONS {
            registry.function(FunctionEntry::sync(name, CTYPE_PARAMS, *function));
        }
    }
}

const FUNCTIONS: &[(&str, SyncFn)] = &[
    ("ctype_alnum", ctype_alnum),
    ("ctype_alpha", ctype_alpha),
    ("ctype_cntrl", ctype_cntrl),
    ("ctype_digit", ctype_digit),
    ("ctype_graph", ctype_graph),
    ("ctype_lower", ctype_lower),
    ("ctype_print", ctype_print),
    ("ctype_punct", ctype_punct),
    ("ctype_space", ctype_space),
    ("ctype_upper", ctype_upper),
    ("ctype_xdigit", ctype_xdigit),
];

/// Legacy integer inputs denote a byte, while non-string inputs emit a deprecation.
fn check(
    context: &mut dyn Context,
    arguments: &[Value],
    name: &str,
    predicate: fn(u8) -> bool,
) -> NativeResult {
    let value = arguments.first().unwrap_or(&Value::Null);
    let value = value.deref();
    if let Value::String(string) = &*value {
        return Ok(Value::Bool(
            !string.is_empty() && string.as_bytes().iter().all(|b| predicate(*b)),
        ));
    }
    context.deprecated(format!(
        "{name}(): Argument of type {} will be interpreted as string in the future",
        value.type_name()
    ));
    let result = match &*value {
        Value::Int(integer) if (-128..=255).contains(integer) => predicate(*integer as u8),
        Value::Int(integer) if *integer >= 0 => predicate(b'0'),
        Value::Int(_) => predicate(b'0') && predicate(b'-'),
        _ => false,
    };
    Ok(Value::Bool(result))
}

// Each class shares PHP's legacy integer conversion and deprecation rules.
macro_rules! class {
    ($function:ident,$name:literal,$predicate:expr) => {
        fn $function(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
            check(context, arguments, $name, $predicate)
        }
    };
}
class!(ctype_alnum, "ctype_alnum", |b| b.is_ascii_alphanumeric());
class!(ctype_alpha, "ctype_alpha", |b| b.is_ascii_alphabetic());
class!(ctype_cntrl, "ctype_cntrl", |b| b.is_ascii_control());
class!(ctype_digit, "ctype_digit", |b| b.is_ascii_digit());
class!(ctype_graph, "ctype_graph", |b| b.is_ascii_graphic());
class!(ctype_lower, "ctype_lower", |b| b.is_ascii_lowercase());
class!(ctype_print, "ctype_print", |b| (32..=126).contains(&b));
class!(ctype_punct, "ctype_punct", |b| b.is_ascii_punctuation());
class!(ctype_space, "ctype_space", |b| matches!(
    b,
    b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c
));
class!(ctype_upper, "ctype_upper", |b| b.is_ascii_uppercase());
class!(ctype_xdigit, "ctype_xdigit", |b| b.is_ascii_hexdigit());
