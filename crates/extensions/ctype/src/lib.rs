//! ASCII character classes matching PHP with LC_CTYPE=C on Linux servers.

use phera_extension_api::{
    Context, Extension, FunctionEntry, NativeResult, Param, ParamType, Registry, SyncFn, Value,
};

pub struct Ctype;
impl Extension for Ctype {
    fn name(&self) -> &'static str {
        "ctype"
    }
    fn register(&self, registry: &mut Registry) {
        const PARAMS: &[Param] = &[Param::new("text", ParamType::Mixed)];
        for (n, f) in FUNCTIONS {
            registry.function(FunctionEntry::sync(n, PARAMS, *f));
        }
    }
}
const FUNCTIONS: &[(&str, SyncFn)] = &[
    ("ctype_alnum", alnum),
    ("ctype_alpha", alpha),
    ("ctype_cntrl", cntrl),
    ("ctype_digit", digit),
    ("ctype_graph", graph),
    ("ctype_lower", lower),
    ("ctype_print", print),
    ("ctype_punct", punct),
    ("ctype_space", space),
    ("ctype_upper", upper),
    ("ctype_xdigit", xdigit),
];
/// Legacy integer inputs denote a byte, while non-string inputs emit a deprecation.
fn check(
    context: &mut dyn Context,
    arguments: &[Value],
    name: &str,
    p: fn(u8) -> bool,
) -> NativeResult {
    let value = arguments.first().unwrap_or(&Value::Null);
    let value = value.deref();
    if let Value::String(string) = &*value {
        return Ok(Value::Bool(
            !string.is_empty() && string.as_bytes().iter().all(|b| p(*b)),
        ));
    }
    context.deprecated(format!(
        "{name}(): Argument of type {} will be interpreted as string in the future",
        value.type_name()
    ));
    let result = match &*value {
        Value::Int(i) if (-128..=255).contains(i) => p(*i as u8),
        Value::Int(i) if *i >= 0 => p(b'0'),
        Value::Int(_) => p(b'0') && p(b'-'),
        _ => false,
    };
    Ok(Value::Bool(result))
}
macro_rules! class {
    ($f:ident,$name:literal,$p:expr) => {
        fn $f(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
            check(context, arguments, $name, $p)
        }
    };
}
class!(alnum, "ctype_alnum", |b| b.is_ascii_alphanumeric());
class!(alpha, "ctype_alpha", |b| b.is_ascii_alphabetic());
class!(cntrl, "ctype_cntrl", |b| b.is_ascii_control());
class!(digit, "ctype_digit", |b| b.is_ascii_digit());
class!(graph, "ctype_graph", |b| b.is_ascii_graphic());
class!(lower, "ctype_lower", |b| b.is_ascii_lowercase());
class!(print, "ctype_print", |b| (32..=126).contains(&b));
class!(punct, "ctype_punct", |b| b.is_ascii_punctuation());
class!(space, "ctype_space", |b| matches!(
    b,
    b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c
));
class!(upper, "ctype_upper", |b| b.is_ascii_uppercase());
class!(xdigit, "ctype_xdigit", |b| b.is_ascii_hexdigit());
