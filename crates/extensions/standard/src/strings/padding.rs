//! Padding and newline insertion preserve the original byte stream.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::{buffer, bytes};

enum Side {
    Left,
    Right,
    Both,
}

const PAD_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("length", ParamType::Int),
    Param::new("pad_string", ParamType::String).optional(),
    Param::new("pad_type", ParamType::Int).optional(),
];

const NEWLINES_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("use_xhtml", ParamType::Bool).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("str_pad", PAD_PARAMS, pad));
    registry.function(FunctionEntry::sync("nl2br", NEWLINES_PARAMS, newlines));
}

/// Padding arguments are checked only if the target is longer than the input.
fn pad(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let length = args::int(arguments, 1).unwrap_or_default();
    if length <= string.len() as i64 {
        return Ok(bytes(string.to_vec()));
    }
    let padding = if args::given(arguments, 2) {
        args::bytes(arguments, 2)
    } else {
        b" "
    };
    if padding.is_empty() {
        return Err(NativeError::value_error(
            "str_pad(): Argument #3 ($pad_string) must not be empty",
        ));
    }
    let side = match args::int(arguments, 3).unwrap_or(1) {
        0 => Side::Left,
        1 => Side::Right,
        2 => Side::Both,
        _ => {
            return Err(NativeError::value_error(
                "str_pad(): Argument #4 ($pad_type) must be STR_PAD_LEFT, STR_PAD_RIGHT, or STR_PAD_BOTH",
            ));
        }
    };
    let mut result = buffer(length as usize)?;
    let extra = length as usize - string.len();
    let left = match side {
        Side::Left => extra,
        Side::Both => extra / 2,
        Side::Right => 0,
    };
    result.extend(padding.iter().cycle().take(left));
    result.extend_from_slice(string);
    result.extend(padding.iter().cycle().take(extra - left));
    Ok(bytes(result))
}

/// CRLF and LFCR each produce one break, and retain both original bytes.
fn newlines(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let tag: &[u8] = if args::bool(arguments, 1).unwrap_or(true) {
        b"<br />"
    } else {
        b"<br>"
    };
    let mut result = buffer(string.len().saturating_mul(7))?;
    let mut cursor = 0;
    while let Some(&byte) = string.get(cursor) {
        if matches!(byte, b'\r' | b'\n') {
            result.extend_from_slice(tag);
            result.push(byte);
            if let Some(&next) = string.get(cursor + 1)
                && matches!(next, b'\r' | b'\n')
                && next != byte
            {
                result.push(next);
                cursor += 1;
            }
        } else {
            result.push(byte);
        }
        cursor += 1;
    }
    Ok(bytes(result))
}
