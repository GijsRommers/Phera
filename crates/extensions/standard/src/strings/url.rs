//! Percent encoding works on bytes, without interpreting a character encoding.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::{buffer, bytes};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Encoding {
    Form,
    Raw,
}

const HEX: &[u8] = b"0123456789ABCDEF";

const STRING_PARAMS: &[Param] = &[Param::new("string", ParamType::String)];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("urlencode", STRING_PARAMS, urlencode));
    registry.function(FunctionEntry::sync(
        "rawurlencode",
        STRING_PARAMS,
        rawurlencode,
    ));
    registry.function(FunctionEntry::sync("urldecode", STRING_PARAMS, urldecode));
    registry.function(FunctionEntry::sync(
        "rawurldecode",
        STRING_PARAMS,
        rawurldecode,
    ));
}

fn urlencode(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    encode(arguments, Encoding::Form)
}

fn rawurlencode(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    encode(arguments, Encoding::Raw)
}

fn urldecode(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    decode(arguments, Encoding::Form)
}

fn rawurldecode(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    decode(arguments, Encoding::Raw)
}

/// Form encoding uses '+' for spaces; RFC 3986 encoding preserves '~'.
fn encode(arguments: &[Value], encoding: Encoding) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len().saturating_mul(3))?;
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
                HEX.get(usize::from(byte >> 4)).copied().unwrap_or_default(),
                HEX.get(usize::from(byte & 15)).copied().unwrap_or_default(),
            ]);
        }
    }
    Ok(bytes(result))
}

pub(super) fn unhex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Malformed percent escapes are copied literally, rather than rejected.
fn decode(arguments: &[Value], encoding: Encoding) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len())?;
    let mut cursor = 0;
    while let Some(&byte) = string.get(cursor) {
        if byte == b'%'
            && let Some(pair) = string.get(cursor + 1..cursor + 3)
            && let (Some(high), Some(low)) = (
                pair.first().copied().and_then(unhex),
                pair.get(1).copied().and_then(unhex),
            )
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
    Ok(bytes(result))
}
