//! Searching returns and takes character positions, not byte offsets:
//!
//! ```php
//! mb_strpos('éabc', 'b');          // 2 (strpos says 3)
//! mb_stripos('ΣΑΣ', 'σ', 1);       // 2: case-insensitive, by simple case folding
//! mb_strrchr('a/b/c', '/');        // "/c"
//! mb_strstr('user@host', '@', true); // "user"
//! mb_substr_count('ababab', 'ab'); // 3
//! ```
//!
//! Like PHP, every encoding is searched as UTF-8: both strings are converted
//! first, with each invalid sequence as the byte 0xFF, which no valid UTF-8
//! contains, so bad bytes only match bad bytes.

use std::borrow::Cow;

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::codec::{BAD, decode};
use crate::encoding::{self, Encoding};
use crate::text;
use crate::unicode::{self, Case};

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("mb_strpos", POSITION, mb_strpos));
    registry.function(FunctionEntry::sync("mb_strrpos", POSITION, mb_strrpos));
    registry.function(FunctionEntry::sync("mb_stripos", POSITION, mb_stripos));
    registry.function(FunctionEntry::sync("mb_strripos", POSITION, mb_strripos));
    registry.function(FunctionEntry::sync("mb_strstr", PORTION, mb_strstr));
    registry.function(FunctionEntry::sync("mb_strrchr", PORTION, mb_strrchr));
    registry.function(FunctionEntry::sync("mb_stristr", PORTION, mb_stristr));
    registry.function(FunctionEntry::sync("mb_strrichr", PORTION, mb_strrichr));
    registry.function(FunctionEntry::sync(
        "mb_substr_count",
        MB_SUBSTR_COUNT,
        mb_substr_count,
    ));
}

#[derive(Clone, Copy)]
enum Search {
    First,
    Last,
    FirstFolded,
    LastFolded,
}

impl Search {
    fn reverse(self) -> bool {
        matches!(self, Self::Last | Self::LastFolded)
    }

    fn folded(self) -> bool {
        matches!(self, Self::FirstFolded | Self::LastFolded)
    }
}

const HAYSTACK: Param = Param::new("haystack", ParamType::String);
const NEEDLE: Param = Param::new("needle", ParamType::String);

const POSITION: &[Param] = &[
    HAYSTACK,
    NEEDLE,
    Param::new("offset", ParamType::Int).optional(),
    crate::ENCODING,
];

/// `mb_strpos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false`
fn mb_strpos(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    position(ctx, a, "mb_strpos", Search::First)
}

/// `mb_strrpos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false`
fn mb_strrpos(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    position(ctx, a, "mb_strrpos", Search::Last)
}

/// `mb_stripos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false`
fn mb_stripos(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    position(ctx, a, "mb_stripos", Search::FirstFolded)
}

/// `mb_strripos(string $haystack, string $needle, int $offset = 0, ?string $encoding = null): int|false`
fn mb_strripos(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    position(ctx, a, "mb_strripos", Search::LastFolded)
}

fn position(ctx: &mut dyn Context, a: &[Value], function: &str, search: Search) -> NativeResult {
    let encoding = encoding::argument(ctx, a, 3, function)?;
    let haystack = searchable(args::bytes(a, 0), encoding, search.folded());
    let needle = searchable(args::bytes(a, 1), encoding, search.folded());
    let offset = args::int(a, 2).unwrap_or(0);
    let found = find(&haystack, &needle, offset, search.reverse()).ok_or_else(|| {
        NativeError::value_error(format!(
            "{function}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
        ))
    })?;
    Ok(found.map_or(Value::Bool(false), |index| Value::Int(index as i64)))
}

const PORTION: &[Param] = &[
    HAYSTACK,
    NEEDLE,
    Param::new("before_needle", ParamType::Bool).optional(),
    crate::ENCODING,
];

/// `mb_strstr(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false`
fn mb_strstr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    portion(ctx, a, "mb_strstr", Search::First)
}

/// `mb_strrchr(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false`
fn mb_strrchr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    portion(ctx, a, "mb_strrchr", Search::Last)
}

/// `mb_stristr(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false`
fn mb_stristr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    portion(ctx, a, "mb_stristr", Search::FirstFolded)
}

/// `mb_strrichr(string $haystack, string $needle, bool $before_needle = false, ?string $encoding = null): string|false`
fn mb_strrichr(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    portion(ctx, a, "mb_strrichr", Search::LastFolded)
}

/// The haystack from the needle on, or before it.
fn portion(ctx: &mut dyn Context, a: &[Value], function: &str, search: Search) -> NativeResult {
    let encoding = encoding::argument(ctx, a, 3, function)?;
    let haystack = args::bytes(a, 0);
    let searched = searchable(haystack, encoding, search.folded());
    let needle = searchable(args::bytes(a, 1), encoding, search.folded());
    let Some(Some(index)) = find(&searched, &needle, 0, search.reverse()) else {
        return Ok(Value::Bool(false));
    };
    let portion = if args::bool(a, 2).unwrap_or(false) {
        text::substring(ctx, haystack, 0, Some(index as i64), encoding)
    } else {
        text::substring(ctx, haystack, index as i64, None, encoding)
    };
    Ok(Value::from(portion.into_owned()))
}

const MB_SUBSTR_COUNT: &[Param] = &[HAYSTACK, NEEDLE, crate::ENCODING];

/// `mb_substr_count(string $haystack, string $needle, ?string $encoding = null): int`:
/// matches do not overlap.
fn mb_substr_count(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (haystack, encoding) = text::decode_argument(ctx, a, 2, "mb_substr_count")?;
    let needle = decode(args::bytes(a, 1), encoding);
    if needle.is_empty() {
        return Err(NativeError::value_error(
            "mb_substr_count(): Argument #2 ($needle) must not be empty",
        ));
    }
    let mut count = 0;
    let mut offset = 0;
    while offset < haystack.len() {
        if haystack[offset..].starts_with(&needle) {
            count += 1;
            offset += needle.len();
        } else {
            offset += 1;
        }
    }
    Ok(Value::Int(count))
}

/// The string as PHP searches it: UTF-8, folded for the case-insensitive
/// functions, invalid sequences as 0xFF.
fn searchable(bytes: &[u8], encoding: Encoding, folded: bool) -> Cow<'_, [u8]> {
    if encoding == Encoding::Utf8 && !folded {
        return Cow::Borrowed(bytes);
    }
    let mut points = decode(bytes, encoding);
    if folded {
        points = unicode::convert(&points, Case::FoldSimple);
    }
    let mut utf8 = Vec::with_capacity(points.len());
    for point in points {
        if point == BAD {
            utf8.push(0xFF);
        } else if let Some(character) = char::from_u32(point) {
            utf8.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
        }
    }
    Cow::Owned(utf8)
}

/// The character position of the needle, `Ok(None)` if it is not there,
/// and `None` if the offset lies outside the haystack.
///
/// A negative offset counts from the end. Searching backwards from a
/// negative offset, as `mb_strrpos` does, a match may start up to there
/// and extend past it.
fn find(haystack: &[u8], needle: &[u8], offset: i64, reverse: bool) -> Option<Option<usize>> {
    let position = byte_offset(haystack, offset)?;
    let (from, end) = if reverse && offset < 0 {
        let extra = byte_offset(&haystack[position..], characters(needle) as i64)
            .unwrap_or(haystack.len() - position);
        (0, position + extra)
    } else {
        (position, haystack.len())
    };
    if needle.is_empty() {
        return Some(Some(characters(
            &haystack[..if reverse { end } else { from }],
        )));
    }
    if needle.len() > end - from {
        return Some(None);
    }
    let mut matches =
        (from..=end - needle.len()).filter(|&index| haystack[index..].starts_with(needle));
    let found = if reverse {
        matches.next_back()
    } else {
        matches.next()
    };
    Some(found.map(|index| characters(&haystack[..index])))
}

fn is_continuation(byte: u8) -> bool {
    byte & 0xC0 == 0x80
}

fn characters(utf8: &[u8]) -> usize {
    utf8.iter().filter(|&&byte| !is_continuation(byte)).count()
}

/// The byte offset of character `offset`, counted from the end when it is
/// negative; `None` when the string is shorter.
fn byte_offset(utf8: &[u8], offset: i64) -> Option<usize> {
    if offset < 0 {
        let wanted = offset.unsigned_abs() as usize;
        return utf8
            .iter()
            .enumerate()
            .rev()
            .filter(|&(_, &byte)| !is_continuation(byte))
            .nth(wanted - 1)
            .map(|(index, _)| index);
    }
    let mut index = 0;
    for _ in 0..offset {
        if index >= utf8.len() {
            return None;
        }
        index += match utf8[index] {
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => 1,
        };
    }
    Some(index.min(utf8.len()))
}
