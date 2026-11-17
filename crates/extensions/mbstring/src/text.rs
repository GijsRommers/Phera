//! Counting and slicing by character instead of by byte:
//!
//! ```php
//! strlen('aéz');               // 4
//! mb_strlen('aéz');            // 3
//! mb_substr('aéz', 1, 1);      // "é"
//! mb_str_split('aéz');         // ["a", "é", "z"]
//! mb_ord('é');                 // 233
//! mb_strlen("\xFF");           // 1: an invalid byte is one character
//! ```

use std::borrow::Cow;

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

use crate::codec::{BAD, Substitute, boundaries, decode, encode};
use crate::encoding::{self, Encoding, Family, Transfer};
use crate::state::Settings;
use crate::{ENCODING, STRING, unicode};

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("mb_strlen", MB_STRLEN, mb_strlen));
    registry.function(FunctionEntry::sync("mb_substr", MB_SUBSTR, mb_substr));
    registry.function(FunctionEntry::sync(
        "mb_str_split",
        MB_STR_SPLIT,
        mb_str_split,
    ));
    registry.function(FunctionEntry::sync("mb_strwidth", MB_STRWIDTH, mb_strwidth));
    registry.function(FunctionEntry::sync("mb_ord", MB_ORD, mb_ord));
    registry.function(FunctionEntry::sync("mb_chr", MB_CHR, mb_chr));
    registry.function(FunctionEntry::sync("mb_scrub", MB_SCRUB, mb_scrub));
}

/// The string argument decoded with the encoding argument at `index`.
pub fn decode_argument(
    ctx: &mut dyn Context,
    a: &[Value],
    index: usize,
    function: &str,
) -> Result<(Vec<u32>, Encoding), NativeError> {
    let encoding = encoding::argument(ctx, a, index, function)?;
    Ok((decode(args::bytes(a, 0), encoding), encoding))
}

/// `points` encoded with the request's substitute character.
pub fn encode_result(ctx: &mut dyn Context, points: &[u32], encoding: Encoding) -> Vec<u8> {
    let substitute = &ctx.state().get_or_default::<Settings>().substitute;
    encode(points, encoding, substitute)
}

const MB_STRLEN: &[Param] = &[STRING, ENCODING];

/// `mb_strlen(string $string, ?string $encoding = null): int`
fn mb_strlen(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let encoding = encoding::argument(ctx, a, 1, "mb_strlen")?;
    Ok(Value::Int(
        character_count(args::bytes(a, 0), encoding) as i64
    ))
}

/// `mb_strlen` for an encoding already resolved.
pub fn character_count(bytes: &[u8], encoding: Encoding) -> usize {
    if encoding == Encoding::Uuencode {
        // PHP counts uuencoded bytes as they are.
        bytes.len()
    } else if let Some(width) = encoding.fixed_width() {
        bytes.len() / width
    } else if encoding == Encoding::Utf8
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        text.chars().count()
    } else {
        decode(bytes, encoding).len()
    }
}

const MB_SUBSTR: &[Param] = &[
    STRING,
    Param::new("start", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
    ENCODING,
];

/// `mb_substr(string $string, int $start, ?int $length = null, ?string $encoding = null): string`
fn mb_substr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    // PHP negates offsets, which `PHP_INT_MIN` does not survive.
    for (index, parameter) in [(1, "start"), (2, "length")] {
        if args::int(a, index) == Some(i64::MIN) {
            return Err(NativeError::value_error(format!(
                "mb_substr(): Argument #{} (${parameter}) must be between -{} and {}",
                index + 1,
                i64::MAX,
                i64::MAX
            )));
        }
    }
    let encoding = encoding::argument(ctx, a, 3, "mb_substr")?;
    let start = args::int(a, 1).unwrap_or(0);
    let substring = substring(ctx, args::bytes(a, 0), start, args::int(a, 2), encoding);
    Ok(Value::from(substring.into_owned()))
}

/// `mb_substr` for an encoding already resolved. Valid UTF-8 and the
/// fixed-width encodings are sliced without decoding, so the result can
/// borrow from `bytes`.
pub fn substring<'a>(
    ctx: &mut dyn Context,
    bytes: &'a [u8],
    start: i64,
    length: Option<i64>,
    encoding: Encoding,
) -> Cow<'a, [u8]> {
    if encoding == Encoding::Utf8
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        return Cow::Borrowed(utf8_substring(text, start, length).as_bytes());
    }
    if let Some(width) = encoding.fixed_width() {
        return Cow::Borrowed(fixed_width_substring(bytes, width, start, length));
    }
    let points = decode(bytes, encoding);
    let (start, end) = bounds(points.len(), start, length);
    Cow::Owned(encode_result(ctx, &points[start..end], encoding))
}

/// Valid UTF-8 is sliced at character boundaries. Counting the characters
/// takes a pass over the whole string, which only negative offsets need.
fn utf8_substring(text: &str, start: i64, length: Option<i64>) -> &str {
    let from_end = start < 0 || length.is_some_and(|length| length < 0);
    let (first, last) = if from_end {
        let (start, end) = bounds(text.chars().count(), start, length);
        let first = byte_offset(text, start);
        (first, first + byte_offset(&text[first..], end - start))
    } else {
        let first = byte_offset(text, start as usize);
        let last = match length {
            None => text.len(),
            Some(length) => first + byte_offset(&text[first..], length as usize),
        };
        (first, last)
    };
    &text[first..last]
}

/// Where character `index` starts, or the end of the text: the offset of
/// the `index`th byte that does not continue a character (`0b10xxxxxx`).
fn byte_offset(text: &str, index: usize) -> usize {
    text.bytes()
        .enumerate()
        .filter(|&(_, byte)| byte & 0xC0 != 0x80)
        .nth(index)
        .map_or(text.len(), |(offset, _)| offset)
}

/// PHP slices fixed-width encodings by bytes, and unlike [`bounds`] lets a
/// positive start run past the end.
fn fixed_width_substring(bytes: &[u8], width: usize, start: i64, length: Option<i64>) -> &[u8] {
    let characters = (bytes.len() / width) as i64;
    let start = if start < 0 {
        characters.saturating_add(start).max(0)
    } else {
        start
    } as usize;
    let first = start.saturating_mul(width).min(bytes.len());
    let length = match length {
        None => bytes.len(),
        Some(n) if n >= 0 => (n as usize).saturating_mul(width),
        Some(n) => {
            characters
                .saturating_sub(start as i64)
                .saturating_add(n)
                .max(0) as usize
                * width
        }
    };
    let end = first.saturating_add(length).min(bytes.len());
    &bytes[first..end]
}

/// The character range `start` and `length` select in a string of `size`
/// characters, with PHP's negative offsets counted from the end.
pub fn bounds(size: usize, start: i64, length: Option<i64>) -> (usize, usize) {
    let size = size as i64;
    let start = if start < 0 {
        size.saturating_add(start).max(0)
    } else {
        start.min(size)
    };
    let end = match length {
        None => size,
        Some(n) if n < 0 => size.saturating_add(n).max(start),
        Some(n) => start.saturating_add(n).min(size),
    };
    (start as usize, end as usize)
}

const MB_STR_SPLIT: &[Param] = &[
    STRING,
    Param::new("length", ParamType::Int).optional(),
    ENCODING,
];

/// `mb_str_split(string $string, int $length = 1, ?string $encoding = null): array`
fn mb_str_split(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let length = args::int(a, 1).unwrap_or(1);
    if length <= 0 {
        return Err(NativeError::value_error(
            "mb_str_split(): Argument #2 ($length) must be greater than 0",
        ));
    }
    let length = length as usize;
    let encoding = encoding::argument(ctx, a, 2, "mb_str_split")?;
    let bytes = args::bytes(a, 0);
    let mut pieces = PhpArray::new();
    match encoding.family() {
        // PHP decodes these and encodes each piece again.
        Family::Utf16(_) | Family::Iso2022Jp | Family::Gb18030 => {
            for chunk in decode(bytes, encoding).chunks(length) {
                pieces.push(Value::from(encode_result(ctx, chunk, encoding)))?;
            }
        }
        // The others are cut at lead bytes, keeping invalid bytes as they are.
        _ => {
            let offsets = boundaries(bytes, encoding);
            let last = offsets.len() - 1;
            for index in (0..last).step_by(length) {
                let next = index.saturating_add(length).min(last);
                let end = offsets[next].min(bytes.len());
                pieces.push(Value::from(&bytes[offsets[index]..end]))?;
            }
        }
    }
    Ok(Value::from(pieces))
}

const MB_STRWIDTH: &[Param] = &[STRING, ENCODING];

/// `mb_strwidth(string $string, ?string $encoding = null): int`: East Asian
/// wide and full-width characters count as two.
fn mb_strwidth(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (points, _) = decode_argument(ctx, a, 1, "mb_strwidth")?;
    let width: usize = points.into_iter().map(unicode::width).sum();
    Ok(Value::Int(width as i64))
}

const MB_ORD: &[Param] = &[STRING, ENCODING];

/// `mb_ord(string $string, ?string $encoding = null): int|false`
fn mb_ord(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (points, encoding) = decode_argument(ctx, a, 1, "mb_ord")?;
    reject_stateful(encoding, "mb_ord")?;
    match points.first() {
        None => Err(NativeError::value_error(
            "mb_ord(): Argument #1 ($string) must not be empty",
        )),
        Some(&BAD) => Ok(Value::Bool(false)),
        Some(&point) => Ok(Value::Int(i64::from(point))),
    }
}

const MB_CHR: &[Param] = &[Param::new("codepoint", ParamType::Int), ENCODING];

/// `mb_chr(int $codepoint, ?string $encoding = null): string|false`
fn mb_chr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let encoding = encoding::argument(ctx, a, 1, "mb_chr")?;
    reject_stateful(encoding, "mb_chr")?;
    let Ok(point @ 0..=0x10FFFF) = u32::try_from(args::int(a, 0).unwrap_or(0)) else {
        return Ok(Value::Bool(false));
    };
    // Lone surrogates are not characters, but the UTF-16, UTF-32, UCS-2 and
    // UCS-4 encoders write them as they are, as PHP does; every other
    // encoder rejects them.
    let bytes = encode(&[point], encoding, &Substitute::None);
    Ok(if bytes.is_empty() {
        Value::Bool(false)
    } else {
        Value::from(bytes)
    })
}

/// `mb_ord` and `mb_chr` work on single characters, which ISO-2022-JP and
/// the transfer encodings do not have.
fn reject_stateful(encoding: Encoding, function: &str) -> Result<(), NativeError> {
    if encoding == Encoding::Iso2022Jp || encoding.transfer().is_some() {
        return Err(NativeError::value_error(format!(
            "{function}() does not support the \"{}\" encoding",
            encoding.name()
        )));
    }
    Ok(())
}

const MB_SCRUB: &[Param] = &[STRING, ENCODING];

/// `mb_scrub(string $string, ?string $encoding = null): string`: invalid
/// sequences become the substitute character.
fn mb_scrub(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (points, encoding) = decode_argument(ctx, a, 1, "mb_scrub")?;
    let scrubbed = match encoding.transfer() {
        // PHP encodes the raw bytes again for these two...
        Some(Transfer::Base64 | Transfer::QuotedPrintable) => {
            let raw: Vec<u32> = args::bytes(a, 0)
                .iter()
                .map(|&byte| u32::from(byte))
                .collect();
            encode_result(ctx, &raw, encoding)
        }
        // ...and returns uuencode decoded.
        Some(Transfer::Uuencode) => points.iter().map(|&point| point as u8).collect(),
        _ => encode_result(ctx, &points, encoding),
    };
    Ok(Value::from(scrubbed))
}
