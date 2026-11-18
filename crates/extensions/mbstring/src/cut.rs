//! Shortening a string to a byte budget or a display width without
//! splitting a character:
//!
//! ```php
//! mb_strcut('éé', 1, 2);              // "é": byte 1 is inside the first é, which starts at 0
//! mb_strimwidth('Hello World', 0, 8, '…'); // "Hello W…"
//! mb_strimwidth('日本語', 0, 5, '');   // "日本": each counts as two
//! ```

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::codec::{Substitute, boundaries, decode, encode};
use crate::encoding::{ByteOrder, Encoding, Family};
use crate::text::{bounds, decode_argument, encode_result};
use crate::{ENCODING, STRING, unicode};

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("mb_strcut", MB_STRCUT, mb_strcut));
    registry.function(FunctionEntry::sync(
        "mb_strimwidth",
        MB_STRIMWIDTH,
        mb_strimwidth,
    ));
}

const MB_STRCUT: &[Param] = &[
    STRING,
    Param::new("start", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
    ENCODING,
];

/// `mb_strcut(string $string, int $start, ?int $length = null, ?string $encoding = null): string`:
/// `$start` and `$length` count bytes; the cut moves back to the start of
/// the character it falls in.
fn mb_strcut(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (points, encoding) = decode_argument(ctx, a, 3, "mb_strcut")?;
    let bytes = args::bytes(a, 0);
    let (start, end) = bounds(bytes.len(), args::int(a, 1).unwrap_or(0), args::int(a, 2));
    if args::int(a, 1).is_some_and(|start| start > bytes.len() as i64) {
        return Ok(Value::from(""));
    }
    let length = match args::int(a, 2) {
        None => bytes.len(),
        Some(length) if length >= 0 => length as usize,
        Some(_) => end.saturating_sub(start),
    };
    let cut = match encoding.family() {
        Family::Utf8 => cut_utf8(bytes, start, length),
        Family::Utf16(order) => cut_utf16(bytes, start, length, order),
        Family::Iso2022Jp => return Ok(Value::from(cut_iso2022jp(ctx, &points, start, end))),
        family => cut_at_boundaries(bytes, start, length, encoding, family),
    };
    Ok(Value::from(cut))
}

fn is_continuation(byte: u8) -> bool {
    byte & 0xC0 == 0x80
}

fn cut_utf8(bytes: &[u8], start: usize, length: usize) -> &[u8] {
    let mut first = start;
    while first > 0 && first < bytes.len() && is_continuation(bytes[first]) {
        first -= 1;
    }
    let mut last = first.saturating_add(length).min(bytes.len());
    while last > first && last < bytes.len() && is_continuation(bytes[last]) {
        last -= 1;
    }
    &bytes[first..last]
}

/// Cuts at even offsets, and not between the two halves of a surrogate pair.
fn cut_utf16(bytes: &[u8], start: usize, length: usize, order: ByteOrder) -> &[u8] {
    let first = start & !1;
    let mut last = first + (length.min(bytes.len() - start) & !1);
    if last >= first + 2 {
        let unit = [bytes[last - 2], bytes[last - 1]];
        let unit = if order == ByteOrder::Little {
            u16::from_le_bytes(unit)
        } else {
            u16::from_be_bytes(unit)
        };
        if (0xD800..=0xDBFF).contains(&unit) {
            last -= 2;
        }
    }
    &bytes[first..last]
}

/// For the other encodings, the character starts follow from lead bytes.
fn cut_at_boundaries(
    bytes: &[u8],
    start: usize,
    length: usize,
    encoding: Encoding,
    family: Family,
) -> &[u8] {
    let offsets = boundaries(bytes, encoding);
    let last_at_most = |limit: usize| {
        offsets
            .iter()
            .copied()
            .take_while(|&offset| offset <= limit)
            .last()
    };
    let first = last_at_most(start).unwrap_or(0);
    // PHP's GB18030 counts the length from the requested start, the others
    // from the start of the character.
    let length = if encoding == Encoding::Gb18030 {
        length.min(bytes.len() - start)
    } else {
        length
    };
    let limit = first.saturating_add(length).min(bytes.len());
    let mut last = if limit >= bytes.len() {
        bytes.len()
    } else {
        last_at_most(limit).unwrap_or(first)
    };
    let unit = match family {
        Family::Ucs4(_) => 4,
        Family::Ucs2(_) => 2,
        _ => 1,
    };
    last -= last % unit;
    &bytes[first.min(last)..last]
}

/// ISO-2022-JP's escape sequences make byte offsets depend on what came
/// before, so PHP counts the bytes of each character encoded on its own.
fn cut_iso2022jp(ctx: &mut dyn Context, points: &[u32], start: usize, end: usize) -> Vec<u8> {
    let mut offset = 0;
    let mut selected = Vec::new();
    for &point in points {
        offset += encode(&[point], Encoding::Iso2022Jp, &Substitute::default()).len();
        if offset > start && offset <= end {
            selected.push(point);
        }
    }
    encode_result(ctx, &selected, Encoding::Iso2022Jp)
}

const MB_STRIMWIDTH: &[Param] = &[
    STRING,
    Param::new("start", ParamType::Int),
    Param::new("width", ParamType::Int),
    Param::new("trim_marker", ParamType::String).optional(),
    ENCODING,
];

/// `mb_strimwidth(string $string, int $start, int $width, string $trim_marker = "",
/// ?string $encoding = null): string`: the string from character `$start`,
/// cut to `$width` columns including the marker, which is added only when
/// something was cut.
fn mb_strimwidth(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (points, encoding) = decode_argument(ctx, a, 4, "mb_strimwidth")?;
    let start = args::int(a, 1).unwrap_or(0);
    let start = if start < 0 {
        (points.len() as i64).saturating_add(start)
    } else {
        start
    };
    if start < 0 || start > points.len() as i64 {
        return Err(NativeError::value_error(
            "mb_strimwidth(): Argument #2 ($start) is out of range",
        ));
    }
    let start = start as usize;
    let width_of = |points: &[u32]| {
        points
            .iter()
            .map(|&point| unicode::width(point))
            .sum::<usize>()
    };
    let mut width = args::int(a, 2).unwrap_or(0);
    if width < 0 {
        ctx.deprecated(
            "mb_strimwidth(): passing a negative integer to argument #3 ($width) is deprecated"
                .into(),
        );
        // A negative width counts back from the end of the string.
        width += width_of(&points) as i64 - width_of(&points[..start]) as i64;
    }
    if width < 0 {
        return Err(NativeError::value_error(
            "mb_strimwidth(): Argument #3 ($width) is out of range",
        ));
    }
    let width = width as usize;
    let rest = &points[start..];
    if width_of(rest) <= width {
        return Ok(Value::from(encode_result(ctx, rest, encoding)));
    }
    let marker = args::bytes(a, 3);
    let budget = width.saturating_sub(width_of(&decode(marker, encoding)));
    let mut used = 0;
    let kept = rest
        .iter()
        .take_while(|&&point| {
            used += unicode::width(point);
            used <= budget
        })
        .count();
    let mut trimmed = encode_result(ctx, &rest[..kept], encoding);
    trimmed.extend_from_slice(marker);
    Ok(Value::from(trimmed))
}
