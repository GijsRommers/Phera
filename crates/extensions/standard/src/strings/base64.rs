//! Base64: `base64_encode("hi")` is `"aGk="`.

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

use super::buffer;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[BASE64_ENCODE, BASE64_DECODE];

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// `base64_encode(string $string): string`, always padded with `=`.
///
/// ```php
/// base64_encode("hi");  // "aGk="
/// base64_encode("hi!"); // "aGkh"
/// ```
const BASE64_ENCODE: FunctionEntry = FunctionEntry::sync(
    "base64_encode",
    &[Param::new("string", ParamType::String)],
    base64_encode,
);

fn base64_encode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len().div_ceil(3).saturating_mul(4))?;
    for chunk in string.chunks(3) {
        // Three bytes are four letters of six bits each; a short last chunk
        // is filled with zero bits and its missing letters with `=`.
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let letters = [
            first >> 2,
            ((first & 0b11) << 4) | (second >> 4),
            ((second & 0b1111) << 2) | (third >> 6),
            third & 0b11_1111,
        ];
        for (index, letter) in letters.into_iter().enumerate() {
            result.push(if index <= chunk.len() {
                ALPHABET[usize::from(letter)]
            } else {
                b'='
            });
        }
    }
    Ok(Value::from(result))
}

/// `base64_decode(string $string, bool $strict = false): string|false`
///
/// By default, bytes outside the alphabet are skipped. In strict mode they
/// make the result `false`, as does anything after padding or a length that
/// no encoding produces; missing padding is still accepted. Whitespace is
/// skipped in both modes.
///
/// ```php
/// base64_decode("aGk=");        // "hi"
/// base64_decode("a G k");       // "hi"
/// base64_decode("aG*k", true);  // false
/// ```
const BASE64_DECODE: FunctionEntry = FunctionEntry::sync(
    "base64_decode",
    &[
        Param::new("string", ParamType::String),
        Param::new("strict", ParamType::Bool).optional(),
    ],
    base64_decode,
);

fn base64_decode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let strict = args::bool(arguments, 1).unwrap_or(false);
    let mut result = buffer(string.len())?;
    let mut padding = 0;
    let mut letters = 0;
    // The bits of the current letter group that do not fill a byte yet.
    let mut pending = 0u8;
    for &byte in string {
        let value = match byte {
            b'=' => {
                padding += 1;
                continue;
            }
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            _ if strict => return Ok(Value::Bool(false)),
            _ => continue,
        };
        if strict && padding > 0 {
            return Ok(Value::Bool(false));
        }
        match letters % 4 {
            0 => pending = value << 2,
            1 => {
                result.push(pending | (value >> 4));
                pending = (value & 0b1111) << 4;
            }
            2 => {
                result.push(pending | (value >> 2));
                pending = (value & 0b11) << 6;
            }
            _ => result.push(pending | value),
        }
        letters += 1;
    }
    let impossible_length =
        letters % 4 == 1 || padding > 0 && (padding > 2 || (letters + padding) % 4 != 0);
    if strict && impossible_length {
        return Ok(Value::Bool(false));
    }
    Ok(Value::from(result))
}
