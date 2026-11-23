//! The `ctype_*` functions: does every byte of a string belong to a class?
//!
//! ```php
//! ctype_digit('123');   // true
//! ctype_digit('');      // false: the empty string is in no class
//! ctype_digit(53);      // true, deprecated: an int from -128 to 255 is a byte ('5')
//! ctype_digit(256);     // true, deprecated: a larger int stands for its digits
//! ```
//!
//! The classes are ASCII, as in PHP's default "C" locale.

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
        for &(name, function) in FUNCTIONS {
            registry.function(FunctionEntry::sync(name, CTYPE_PARAMS, function));
        }
    }
}

/// Each function with the class of bytes it accepts. A native function is
/// not told its own name, so each entry passes it on for the deprecation.
const FUNCTIONS: &[(&str, SyncFn)] = &[
    ("ctype_alnum", |c, a| {
        check(c, a, "ctype_alnum", u8::is_ascii_alphanumeric)
    }),
    ("ctype_alpha", |c, a| {
        check(c, a, "ctype_alpha", u8::is_ascii_alphabetic)
    }),
    ("ctype_cntrl", |c, a| {
        check(c, a, "ctype_cntrl", u8::is_ascii_control)
    }),
    ("ctype_digit", |c, a| {
        check(c, a, "ctype_digit", u8::is_ascii_digit)
    }),
    ("ctype_graph", |c, a| {
        check(c, a, "ctype_graph", u8::is_ascii_graphic)
    }),
    ("ctype_lower", |c, a| {
        check(c, a, "ctype_lower", u8::is_ascii_lowercase)
    }),
    ("ctype_print", |c, a| check(c, a, "ctype_print", is_print)),
    ("ctype_punct", |c, a| {
        check(c, a, "ctype_punct", u8::is_ascii_punctuation)
    }),
    ("ctype_space", |c, a| check(c, a, "ctype_space", is_space)),
    ("ctype_upper", |c, a| {
        check(c, a, "ctype_upper", u8::is_ascii_uppercase)
    }),
    ("ctype_xdigit", |c, a| {
        check(c, a, "ctype_xdigit", u8::is_ascii_hexdigit)
    }),
];

/// A graphic character or the space.
fn is_print(byte: &u8) -> bool {
    (32..=126).contains(byte)
}

/// Space, tab, newline, carriage return, vertical tab, form feed. Unlike
/// `u8::is_ascii_whitespace`, this includes the vertical tab.
fn is_space(byte: &u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// Whether the argument, a string, is non-empty and all of its bytes are in
/// the class. Anything else is deprecated: an int from -128 to 255 is
/// checked as one byte (a negative one plus 256), a larger int as its
/// decimal digits (with a `-` when negative), and other types are false.
fn check(
    context: &mut dyn Context,
    arguments: &[Value],
    name: &str,
    in_class: fn(&u8) -> bool,
) -> NativeResult {
    let value = arguments.first().unwrap_or(&Value::Null).deref();
    if let Some(string) = value.as_str_bytes() {
        return Ok(Value::Bool(
            !string.is_empty() && string.iter().all(in_class),
        ));
    }
    context.deprecated(format!(
        "{name}(): Argument of type {} will be interpreted as string in the future",
        value.type_name()
    ));
    let result = match &*value {
        Value::Int(integer) if (-128..=255).contains(integer) => in_class(&(*integer as u8)),
        // Every class that holds one digit holds all ten.
        Value::Int(integer) if *integer >= 0 => in_class(&b'0'),
        Value::Int(_) => in_class(&b'0') && in_class(&b'-'),
        _ => false,
    };
    Ok(Value::Bool(result))
}
