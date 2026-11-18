//! Removing and adding characters at the ends, counted in characters:
//!
//! ```php
//! mb_trim("\u{3000}text\u{3000}");     // "text": Unicode spaces, not only ASCII ones
//! mb_ltrim('ééa', 'é');               // "a"
//! mb_str_pad('é', 3, '-');            // "é--" (str_pad would give "é-")
//! mb_str_pad('é', 4, 'ab', STR_PAD_BOTH); // "aéab"
//! ```

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::codec::decode;
use crate::encoding::Encoding;
use crate::text::{character_count, decode_argument, substring};
use crate::{ENCODING, STRING};

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("mb_trim", TRIM, mb_trim));
    registry.function(FunctionEntry::sync("mb_ltrim", TRIM, mb_ltrim));
    registry.function(FunctionEntry::sync("mb_rtrim", TRIM, mb_rtrim));
    registry.function(FunctionEntry::sync("mb_str_pad", MB_STR_PAD, mb_str_pad));
}

/// What PHP trims by default: NUL, the ASCII and Unicode spaces.
const SPACES: &[u32] = &[
    0x00, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x20, 0x85, 0xA0, 0x1680, 0x2000, 0x2001, 0x2002, 0x2003,
    0x2004, 0x2005, 0x2006, 0x2007, 0x2008, 0x2009, 0x200A, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Both,
    Left,
    Right,
}

const TRIM: &[Param] = &[
    STRING,
    Param::new("characters", ParamType::String)
        .nullable()
        .optional(),
    ENCODING,
];

/// `mb_trim(string $string, ?string $characters = null, ?string $encoding = null): string`
fn mb_trim(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    trim(ctx, a, "mb_trim", Side::Both)
}

/// `mb_ltrim(string $string, ?string $characters = null, ?string $encoding = null): string`
fn mb_ltrim(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    trim(ctx, a, "mb_ltrim", Side::Left)
}

/// `mb_rtrim(string $string, ?string $characters = null, ?string $encoding = null): string`
fn mb_rtrim(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    trim(ctx, a, "mb_rtrim", Side::Right)
}

fn trim(ctx: &mut dyn Context, a: &[Value], function: &str, side: Side) -> NativeResult {
    let (points, encoding) = decode_argument(ctx, a, 2, function)?;
    let custom = args::string(a, 1).map(|characters| decode(characters.as_bytes(), encoding));
    let trimmed = custom.as_deref().unwrap_or(SPACES);
    let kept = |point: &u32| !trimmed.contains(point);
    let start = match side {
        Side::Right => 0,
        _ => points.iter().position(kept).unwrap_or(points.len()),
    };
    let end = match side {
        Side::Left => points.len(),
        _ => points
            .iter()
            .rposition(kept)
            .map_or(start, |index| index + 1),
    };
    if start == 0 && end == points.len() {
        return Ok(args::value(a, 0).clone());
    }
    let length = Some((end - start) as i64);
    let trimmed = substring(ctx, args::bytes(a, 0), start as i64, length, encoding);
    Ok(Value::from(trimmed.into_owned()))
}

const MB_STR_PAD: &[Param] = &[
    STRING,
    Param::new("length", ParamType::Int),
    Param::new("pad_string", ParamType::String).optional(),
    Param::new("pad_type", ParamType::Int).optional(),
    ENCODING,
];

/// `STR_PAD_LEFT`, `STR_PAD_RIGHT` and `STR_PAD_BOTH`.
fn pad_side(pad_type: i64) -> Option<Side> {
    match pad_type {
        0 => Some(Side::Left),
        1 => Some(Side::Right),
        2 => Some(Side::Both),
        _ => None,
    }
}

/// `mb_str_pad(string $string, int $length, string $pad_string = " ",
/// int $pad_type = STR_PAD_RIGHT, ?string $encoding = null): string`
fn mb_str_pad(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let (points, encoding) = decode_argument(ctx, a, 4, "mb_str_pad")?;
    let length = args::int(a, 1).unwrap_or(0);
    if length <= points.len() as i64 {
        return Ok(args::value(a, 0).clone());
    }
    let padding = args::string(a, 2).map_or(&b" "[..], |padding| padding.as_bytes());
    let padding_length = character_count(padding, encoding);
    if padding_length == 0 {
        return Err(NativeError::value_error(
            "mb_str_pad(): Argument #3 ($pad_string) must not be empty",
        ));
    }
    let Some(side) = pad_side(args::int(a, 3).unwrap_or(1)) else {
        return Err(NativeError::value_error(concat!(
            "mb_str_pad(): Argument #4 ($pad_type) ",
            "must be STR_PAD_LEFT, STR_PAD_RIGHT, or STR_PAD_BOTH"
        )));
    };
    let missing = length as usize - points.len();
    let left = match side {
        Side::Left => missing,
        Side::Both => missing / 2,
        Side::Right => 0,
    };
    let mut output = Vec::new();
    output
        .try_reserve(length as usize)
        .map_err(|_| NativeError::error("Allowed memory size exhausted"))?;
    let pad = Padding {
        bytes: padding,
        length: padding_length,
        encoding,
    };
    pad.append(ctx, &mut output, left);
    output.extend_from_slice(args::bytes(a, 0));
    pad.append(ctx, &mut output, missing - left);
    Ok(Value::from(output))
}

struct Padding<'a> {
    bytes: &'a [u8],
    /// In characters.
    length: usize,
    encoding: Encoding,
}

impl Padding<'_> {
    /// Appends `count` characters of padding: whole copies, then the start
    /// of one more.
    fn append(&self, ctx: &mut dyn Context, output: &mut Vec<u8>, count: usize) {
        for _ in 0..count / self.length {
            output.extend_from_slice(self.bytes);
        }
        let rest = Some((count % self.length) as i64);
        output.extend_from_slice(&substring(ctx, self.bytes, 0, rest, self.encoding));
    }
}
