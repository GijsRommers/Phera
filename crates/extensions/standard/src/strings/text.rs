//! Byte-level wrapping, word extraction, and character frequencies.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use super::{buffer, bytes};

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordFormat {
    Count,
    List,
    Offsets,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LongWords {
    Cut,
    Preserve,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CharacterOutput {
    AllCounts,
    PresentCounts,
    AbsentCounts,
    PresentBytes,
    AbsentBytes,
}

const WRAP_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("width", ParamType::Int).optional(),
    Param::new("break", ParamType::String).optional(),
    Param::new("cut_long_words", ParamType::Bool).optional(),
];

const CHUNK_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("length", ParamType::Int).optional(),
    Param::new("separator", ParamType::String).optional(),
];

const WORDS_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("format", ParamType::Int).optional(),
    Param::new("characters", ParamType::String)
        .nullable()
        .optional(),
];

const COUNTS_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("mode", ParamType::Int).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("wordwrap", WRAP_PARAMS, wrap));
    registry.function(FunctionEntry::sync("chunk_split", CHUNK_PARAMS, chunk));
    registry.function(FunctionEntry::sync("str_word_count", WORDS_PARAMS, words));
    registry.function(FunctionEntry::sync("count_chars", COUNTS_PARAMS, counts));
}

/// Even an empty string receives one trailing separator.
fn chunk(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let length = args::int(arguments, 1).unwrap_or(76);
    if length <= 0 {
        return Err(NativeError::value_error(
            "chunk_split(): Argument #2 ($length) must be greater than 0",
        ));
    }
    let separator = if args::given(arguments, 2) {
        args::bytes(arguments, 2)
    } else {
        b"\r\n"
    };
    let count = string.len().div_ceil(length as usize).max(1);
    let mut result = buffer(
        string
            .len()
            .saturating_add(separator.len().saturating_mul(count)),
    )?;
    if string.is_empty() {
        result.extend_from_slice(separator);
    } else {
        for chunk in string.chunks(length as usize) {
            result.extend_from_slice(chunk);
            result.extend_from_slice(separator);
        }
    }
    Ok(bytes(result))
}

/// Existing breaks reset wrapping; only ASCII space is a word boundary.
fn wrap(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    if string.is_empty() {
        return Ok(bytes(Vec::new()));
    }
    let width = args::int(arguments, 1).unwrap_or(75);
    let separator = if args::given(arguments, 2) {
        args::bytes(arguments, 2)
    } else {
        b"\n"
    };
    let cut = if args::bool(arguments, 3).unwrap_or(false) {
        LongWords::Cut
    } else {
        LongWords::Preserve
    };
    if separator.is_empty() {
        return Err(NativeError::value_error(
            "wordwrap(): Argument #3 ($break) must not be empty",
        ));
    }
    if width == 0 && cut == LongWords::Cut {
        return Err(NativeError::value_error(
            "wordwrap(): Argument #4 ($cut_long_words) cannot be true when argument #2 ($width) is 0",
        ));
    }
    wrap_bytes(string, width, separator, cut)
}

/// Existing separators and chosen spaces reset the byte-column counter.
fn wrap_bytes(string: &[u8], width: i64, separator: &[u8], cut: LongWords) -> NativeResult {
    let mut result = buffer(
        string
            .len()
            .saturating_mul(separator.len().saturating_add(1)),
    )?;
    let mut start = 0;
    let mut space = 0;
    let mut cursor = 0;
    while let Some(&byte) = string.get(cursor) {
        if string
            .get(cursor..)
            .is_some_and(|tail| tail.starts_with(separator))
        {
            let end = cursor + separator.len();
            result.extend_from_slice(string.get(start..end).unwrap_or_default());
            cursor = end;
            start = end;
            space = end;
            continue;
        }
        let over = (cursor.saturating_sub(start) as i64) >= width;
        if byte == b' ' {
            if over {
                result.extend_from_slice(string.get(start..cursor).unwrap_or_default());
                result.extend_from_slice(separator);
                start = cursor + 1;
            }
            space = cursor;
        } else if over && cut == LongWords::Cut && start >= space {
            result.extend_from_slice(string.get(start..cursor).unwrap_or_default());
            result.extend_from_slice(separator);
            start = cursor;
            space = cursor;
        } else if over && start < space {
            result.extend_from_slice(string.get(start..space).unwrap_or_default());
            result.extend_from_slice(separator);
            start = space + 1;
            space = start;
        }
        cursor += 1;
    }
    result.extend_from_slice(string.get(start..).unwrap_or_default());
    Ok(bytes(result))
}

/// Only the string's outermost apostrophe/hyphen has special boundary handling.
fn words(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let format = match args::int(arguments, 1).unwrap_or_default() {
        0 => WordFormat::Count,
        1 => WordFormat::List,
        2 => WordFormat::Offsets,
        _ => {
            return Err(NativeError::value_error(
                "str_word_count(): Argument #2 ($format) must be a valid format value",
            ));
        }
    };
    let mut result = PhpArray::new();
    if string.is_empty() {
        return Ok(if format == WordFormat::Count {
            Value::Int(0)
        } else {
            Value::from(result)
        });
    }
    let mask = super::trim::character_mask(context, "str_word_count", args::bytes(arguments, 2));
    let (mut cursor, end) = word_bounds(string, &mask);
    let mut count = 0;
    while cursor < end {
        let start = cursor;
        while cursor < end
            && string.get(cursor).is_some_and(|&byte| {
                super::alphabetic(byte) || matches!(byte, b'\'' | b'-') || mask[usize::from(byte)]
            })
        {
            cursor += 1;
        }
        if cursor > start {
            if format != WordFormat::Count {
                result.insert(
                    ArrayKey::Int(if format == WordFormat::List {
                        count
                    } else {
                        start as i64
                    }),
                    bytes(string.get(start..cursor).unwrap_or_default().to_vec()),
                );
            }
            count += 1;
        }
        cursor += 1;
    }
    Ok(if format == WordFormat::Count {
        Value::Int(count)
    } else {
        Value::from(result)
    })
}

/// Frequency arrays and unique-byte strings are always sorted by byte value.
fn counts(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mode = match args::int(arguments, 1).unwrap_or_default() {
        0 => CharacterOutput::AllCounts,
        1 => CharacterOutput::PresentCounts,
        2 => CharacterOutput::AbsentCounts,
        3 => CharacterOutput::PresentBytes,
        4 => CharacterOutput::AbsentBytes,
        _ => {
            return Err(NativeError::value_error(
                "count_chars(): Argument #2 ($mode) must be between 0 and 4 (inclusive)",
            ));
        }
    };
    let mut counts = [0i64; 256];
    for &byte in args::bytes(arguments, 0) {
        counts[usize::from(byte)] += 1;
    }
    let mut array = PhpArray::new();
    let mut string = buffer(256)?;
    for (byte, &count) in counts.iter().enumerate() {
        if mode.includes(count) {
            if mode.returns_counts() {
                array.insert(ArrayKey::Int(byte as i64), Value::Int(count));
            } else {
                string.push(byte as u8);
            }
        }
    }
    Ok(if mode.returns_counts() {
        Value::from(array)
    } else {
        bytes(string)
    })
}

/// Only outer apostrophes and hyphens receive PHP's special word-boundary treatment.
fn word_bounds(string: &[u8], mask: &[bool; 256]) -> (usize, usize) {
    let mut cursor = 0;
    let mut end = string.len();
    if string
        .first()
        .is_some_and(|&byte| matches!(byte, b'\'' | b'-') && !mask[usize::from(byte)])
    {
        cursor += 1;
    }
    if string.last() == Some(&b'-') && !mask[usize::from(b'-')] {
        end = end.saturating_sub(1);
    }
    (cursor, end)
}

impl CharacterOutput {
    fn includes(self, count: i64) -> bool {
        match self {
            Self::AllCounts => true,
            Self::PresentCounts | Self::PresentBytes => count != 0,
            Self::AbsentCounts | Self::AbsentBytes => count == 0,
        }
    }

    fn returns_counts(self) -> bool {
        matches!(
            self,
            Self::AllCounts | Self::PresentCounts | Self::AbsentCounts
        )
    }
}
