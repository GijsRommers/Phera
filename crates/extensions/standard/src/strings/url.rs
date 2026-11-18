//! Percent encoding: `urlencode("a b&c")` is `"a+b%26c"`.
//!
//! These work on bytes: a UTF-8 `é` is two bytes and becomes `%C3%A9`.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

use super::buffer;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[URLENCODE, RAWURLENCODE, URLDECODE, RAWURLDECODE];

/// Which of PHP's two percent encodings.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    /// `application/x-www-form-urlencoded`: a space is `+`.
    Form,
    /// RFC 3986: a space is `%20`, and `~` stays as it is.
    Raw,
}

/// The value of a hexadecimal digit, either case.
pub(super) fn hex_digit(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).map(|digit| digit as u8)
}

/// `urlencode(string $string): string`, for a query string.
///
/// ```php
/// urlencode("a b&c~"); // "a+b%26c%7E"
/// ```
const URLENCODE: FunctionEntry = FunctionEntry::sync(
    "urlencode",
    &[Param::new("string", ParamType::String)],
    urlencode,
);

fn urlencode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    encode(args::bytes(arguments, 0), Encoding::Form)
}

/// `rawurlencode(string $string): string`, for a URL path.
///
/// ```php
/// rawurlencode("a b&c~"); // "a%20b%26c~"
/// ```
const RAWURLENCODE: FunctionEntry = FunctionEntry::sync(
    "rawurlencode",
    &[Param::new("string", ParamType::String)],
    rawurlencode,
);

fn rawurlencode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    encode(args::bytes(arguments, 0), Encoding::Raw)
}

/// `urldecode(string $string): string`
///
/// A `%` not followed by two hexadecimal digits is kept as it is.
///
/// ```php
/// urldecode("a+b%26c%zz"); // "a b&c%zz"
/// ```
const URLDECODE: FunctionEntry = FunctionEntry::sync(
    "urldecode",
    &[Param::new("string", ParamType::String)],
    urldecode,
);

fn urldecode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    decode(args::bytes(arguments, 0), Encoding::Form)
}

/// `rawurldecode(string $string): string`, which leaves `+` alone.
///
/// ```php
/// rawurldecode("a+b%20c"); // "a+b c"
/// ```
const RAWURLDECODE: FunctionEntry = FunctionEntry::sync(
    "rawurldecode",
    &[Param::new("string", ParamType::String)],
    rawurldecode,
);

fn rawurldecode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    decode(args::bytes(arguments, 0), Encoding::Raw)
}

fn encode(string: &[u8], encoding: Encoding) -> NativeResult {
    let mut result = buffer(string.len().saturating_mul(3))?;
    encode_into(&mut result, string, encoding);
    Ok(Value::from(result))
}

pub(crate) fn encode_into(result: &mut Vec<u8>, string: &[u8], encoding: Encoding) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &byte in string {
        if byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.')
            || (encoding == Encoding::Raw && byte == b'~')
        {
            result.push(byte);
        } else if encoding == Encoding::Form && byte == b' ' {
            result.push(b'+');
        } else {
            result.extend_from_slice(&[
                b'%',
                HEX[usize::from(byte >> 4)],
                HEX[usize::from(byte & 0xf)],
            ]);
        }
    }
}

fn decode(string: &[u8], encoding: Encoding) -> NativeResult {
    let mut result = buffer(string.len())?;
    let mut cursor = 0;
    while cursor < string.len() {
        let byte = string[cursor];
        if byte == b'%'
            && let Some(&[high, low]) = string.get(cursor + 1..cursor + 3)
            && let (Some(high), Some(low)) = (hex_digit(high), hex_digit(low))
        {
            result.push(high * 16 + low);
            cursor += 3;
            continue;
        }
        result.push(if encoding == Encoding::Form && byte == b'+' {
            b' '
        } else {
            byte
        });
        cursor += 1;
    }
    Ok(Value::from(result))
}
