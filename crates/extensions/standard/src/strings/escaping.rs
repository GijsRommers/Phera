//! PHP backslash escaping treats strings and character masks as bytes.
use super::{buffer, bytes};
use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, SyncFn, Value, args,
};
const STRING: &[Param] = &[Param::new("string", ParamType::String)];
const MASK: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("characters", ParamType::String),
];
pub(crate) fn register(registry: &mut Registry) {
    for (name, body) in [
        ("addslashes", addslashes as SyncFn),
        ("stripslashes", stripslashes),
        ("stripcslashes", stripcslashes),
        ("quotemeta", quotemeta),
    ] {
        registry.function(FunctionEntry::sync(name, STRING, body));
    }
    registry.function(FunctionEntry::sync("addcslashes", MASK, addcslashes));
}
/// NUL is escaped as backslash-zero; other quote bytes retain their spelling.
fn addslashes(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len().saturating_mul(2))?;
    for &byte in string {
        if matches!(byte, 0 | b'\'' | b'"' | b'\\') {
            result.push(b'\\');
        }
        result.push(if byte == 0 { b'0' } else { byte });
    }
    Ok(bytes(result))
}
/// A terminal backslash disappears, while backslash-zero becomes a NUL byte.
fn stripslashes(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len())?;
    let mut source = string.iter();
    while let Some(&byte) = source.next() {
        if byte == b'\\' {
            if let Some(&next) = source.next() {
                result.push(if next == b'0' { 0 } else { next });
            }
        } else {
            result.push(byte);
        }
    }
    Ok(bytes(result))
}
/// quotemeta escapes only PHP's fixed punctuation set, including embedded NUL.
fn quotemeta(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len().saturating_mul(2))?;
    for &byte in string {
        if b".\\+*?[^]($)".contains(&byte) {
            result.push(b'\\');
        }
        result.push(byte);
    }
    Ok(bytes(result))
}
/// Non-printable masked bytes use named C escapes or exactly three octal digits.
fn addcslashes(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    if string.is_empty() {
        return Ok(bytes(Vec::new()));
    }
    let mask = super::trim::mask(context, args::bytes(arguments, 1), "addcslashes");
    let mut result = buffer(string.len().saturating_mul(4))?;
    for &byte in string {
        if !mask[usize::from(byte)] {
            result.push(byte);
            continue;
        }
        result.push(b'\\');
        let escaped = match byte {
            b'\n' => Some(b'n'),
            b'\r' => Some(b'r'),
            b'\t' => Some(b't'),
            7 => Some(b'a'),
            11 => Some(b'v'),
            8 => Some(b'b'),
            12 => Some(b'f'),
            _ => None,
        };
        if let Some(escaped) = escaped {
            result.push(escaped);
        } else if (32..=126).contains(&byte) {
            result.push(byte);
        } else {
            result.extend_from_slice(&[
                b'0' + (byte >> 6),
                b'0' + ((byte >> 3) & 7),
                b'0' + (byte & 7),
            ]);
        }
    }
    Ok(bytes(result))
}
/// Octal escapes consume at most three digits and wrap to a single byte.
fn stripcslashes(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mut result = buffer(string.len())?;
    let mut cursor = 0;
    while let Some(&byte) = string.get(cursor) {
        cursor += 1;
        if byte != b'\\' {
            result.push(byte);
            continue;
        }
        let Some(&next) = string.get(cursor) else {
            result.push(byte);
            break;
        };
        cursor += 1;
        let named = match next {
            b'n' => Some(10),
            b'r' => Some(13),
            b't' => Some(9),
            b'a' => Some(7),
            b'v' => Some(11),
            b'b' => Some(8),
            b'f' => Some(12),
            _ => None,
        };
        if let Some(value) = named {
            result.push(value);
            continue;
        }
        if next == b'x'
            && string
                .get(cursor)
                .copied()
                .and_then(super::url::unhex)
                .is_some()
        {
            let mut number = 0u8;
            for _ in 0..2 {
                let Some(digit) = string.get(cursor).copied().and_then(super::url::unhex) else {
                    break;
                };
                number = number.wrapping_mul(16).wrapping_add(digit);
                cursor += 1;
            }
            result.push(number);
        } else if (b'0'..=b'7').contains(&next) {
            let mut number = next - b'0';
            for _ in 0..2 {
                let Some(&digit) = string
                    .get(cursor)
                    .filter(|byte| (b'0'..=b'7').contains(byte))
                else {
                    break;
                };
                number = number.wrapping_mul(8).wrapping_add(digit - b'0');
                cursor += 1;
            }
            result.push(number);
        } else {
            result.push(next);
        }
    }
    Ok(bytes(result))
}
