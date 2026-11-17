//! Binary-safe Base64 with PHP's permissive and strict decoding modes.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::{buffer, bytes};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Decoding {
    Strict,
    Permissive,
}

const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const ENCODE_PARAMS: &[Param] = &[Param::new("string", ParamType::String)];
const DECODE_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("strict", ParamType::Bool).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("base64_encode", ENCODE_PARAMS, encode));
    registry.function(FunctionEntry::sync("base64_decode", DECODE_PARAMS, decode));
}

/// Encoding always emits padding for the final incomplete three-byte group.
fn encode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len().div_ceil(3).saturating_mul(4))?;
    for chunk in string.chunks(3) {
        let first = chunk.first().copied().unwrap_or_default();
        let second = chunk.get(1).copied().unwrap_or_default();
        let third = chunk.get(2).copied().unwrap_or_default();
        for index in [first >> 2, ((first & 3) << 4) | (second >> 4)] {
            result.push(
                ALPHABET
                    .get(usize::from(index))
                    .copied()
                    .unwrap_or_default(),
            );
        }
        result.push(if chunk.len() > 1 {
            ALPHABET
                .get(usize::from(((second & 15) << 2) | (third >> 6)))
                .copied()
                .unwrap_or_default()
        } else {
            b'='
        });
        result.push(if chunk.len() > 2 {
            ALPHABET
                .get(usize::from(third & 63))
                .copied()
                .unwrap_or_default()
        } else {
            b'='
        });
    }
    Ok(bytes(result))
}

/// Strict mode accepts absent padding but rejects data after any padding.
fn decode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mode = if args::bool(arguments, 1).unwrap_or(false) {
        Decoding::Strict
    } else {
        Decoding::Permissive
    };
    let mut result = buffer(string.len())?;
    let mut padding = 0;
    let mut count = 0;
    let mut pending = 0u8;
    for &byte in string {
        if byte == b'=' {
            padding += 1;
            continue;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ => {
                if mode == Decoding::Strict {
                    return Ok(Value::Bool(false));
                }
                continue;
            }
        };
        if mode == Decoding::Strict && padding > 0 {
            return Ok(Value::Bool(false));
        }
        match count % 4 {
            0 => pending = value << 2,
            1 => {
                result.push(pending | (value >> 4));
                pending = (value & 15) << 4;
            }
            2 => {
                result.push(pending | (value >> 2));
                pending = (value & 3) << 6;
            }
            _ => result.push(pending | value),
        }
        count += 1;
    }
    if mode == Decoding::Strict
        && (count % 4 == 1 || padding > 0 && (padding > 2 || (count + padding) % 4 != 0))
    {
        return Ok(Value::Bool(false));
    }
    Ok(bytes(result))
}
