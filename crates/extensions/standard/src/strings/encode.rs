//! Byte-to-hex conversion and the unsigned IEEE CRC-32 checksum.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::{buffer, bytes, url::unhex};

const HEX: &[u8] = b"0123456789abcdef";

const STRING_PARAMS: &[Param] = &[Param::new("string", ParamType::String)];
const ORD_PARAMS: &[Param] = &[Param::new("character", ParamType::String)];
const CHR_PARAMS: &[Param] = &[Param::new("codepoint", ParamType::Int)];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("bin2hex", STRING_PARAMS, bin2hex));
    registry.function(FunctionEntry::sync("hex2bin", STRING_PARAMS, hex2bin));
    registry.function(FunctionEntry::sync("crc32", STRING_PARAMS, crc32));
    registry.function(FunctionEntry::sync("ord", ORD_PARAMS, ord));
    registry.function(FunctionEntry::sync("chr", CHR_PARAMS, chr));
}

/// PHP encodes every byte, including NUL, with lowercase hexadecimal digits.
fn bin2hex(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len().saturating_mul(2))?;
    for &byte in string {
        result.push(HEX.get(usize::from(byte >> 4)).copied().unwrap_or_default());
        result.push(HEX.get(usize::from(byte & 15)).copied().unwrap_or_default());
    }
    Ok(bytes(result))
}

/// PHP reports malformed input as a warning and false, not an exception.
fn hex2bin(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    if !string.len().is_multiple_of(2) {
        context.warning("hex2bin(): Hexadecimal input string must have an even length".into());
        return Ok(Value::Bool(false));
    }
    let mut result = buffer(string.len() / 2)?;
    for pair in string.as_chunks::<2>().0 {
        let (Some(high), Some(low)) = (
            pair.first().copied().and_then(unhex),
            pair.get(1).copied().and_then(unhex),
        ) else {
            context.warning("hex2bin(): Input string must be hexadecimal string".into());
            return Ok(Value::Bool(false));
        };
        result.push(high * 16 + low);
    }
    Ok(bytes(result))
}

/// The reflected polynomial produces PHP's unsigned result on 64-bit builds.
fn crc32(_context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut checksum = u32::MAX;
    for &byte in args::bytes(arguments, 0) {
        checksum ^= u32::from(byte);
        for _ in 0..8 {
            checksum = (checksum >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(checksum & 1));
        }
    }
    Ok(Value::Int(i64::from(!checksum)))
}

/// PHP 8.5 deprecates inputs whose byte length differs from one.
fn ord(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    match args::bytes(arguments, 0).len() {
        0 => context.deprecated("ord(): Providing an empty string is deprecated".into()),
        1 => {},
        _ => context.deprecated("ord(): Providing a string that is not one byte long is deprecated. Use ord($str[0]) instead".into()),
    }
    Ok(Value::Int(i64::from(
        args::bytes(arguments, 0).first().copied().unwrap_or(0),
    )))
}

/// Only the low byte is used, including for negative integers.
fn chr(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    if !(0..=255).contains(&args::int(arguments, 0).unwrap_or_default()) {
        context.deprecated("chr(): Providing a value not in-between 0 and 255 is deprecated, this is because a byte value must be in the [0, 255] interval. The value used will be constrained using % 256".into());
    }
    Ok(bytes(vec![
        args::int(arguments, 0).unwrap_or_default() as u8
    ]))
}
