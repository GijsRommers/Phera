//! Text layout and statistics: `wordwrap`, `chunk_split`, `str_word_count`
//! and `count_chars`.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray, Value,
    args,
};

use super::trim::character_mask;
use super::{alphabetic, buffer};
use crate::arrays::list;

pub(super) const FUNCTIONS: &[FunctionEntry] =
    &[WORDWRAP, CHUNK_SPLIT, STR_WORD_COUNT, COUNT_CHARS];

/// `wordwrap(string $string, int $width = 75, string $break = "\n",
/// bool $cut_long_words = false): string`
///
/// Lines are broken at spaces, counting bytes; a break already in the text
/// starts a new line. A word longer than the width stays whole unless
/// `$cut_long_words` is true.
///
/// ```php
/// wordwrap("The quick brown fox", 10);          // "The quick\nbrown fox"
/// wordwrap("A veryveryverylongword", 8, "\n", true); // "A\nveryvery\nverylong\nword"
/// ```
const WORDWRAP: FunctionEntry = FunctionEntry::sync(
    "wordwrap",
    &[
        Param::new("string", ParamType::String),
        Param::new("width", ParamType::Int).optional(),
        Param::new("break", ParamType::String).optional(),
        Param::new("cut_long_words", ParamType::Bool).optional(),
    ],
    wordwrap,
);

fn wordwrap(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    // An empty string is returned before the other arguments are checked.
    if string.is_empty() {
        return Ok(Value::from(""));
    }
    let width = args::int(arguments, 1).unwrap_or(75);
    let line_break = if args::given(arguments, 2) {
        args::bytes(arguments, 2)
    } else {
        b"\n"
    };
    let cut = args::bool(arguments, 3).unwrap_or(false);
    if line_break.is_empty() {
        return Err(NativeError::value_error(
            "wordwrap(): Argument #3 ($break) must not be empty",
        ));
    }
    if width == 0 && cut {
        return Err(NativeError::value_error(
            "wordwrap(): Argument #4 ($cut_long_words) cannot be true when argument #2 ($width) is 0",
        ));
    }
    let mut result = buffer(string.len().saturating_mul(line_break.len() + 1))?;
    // The current line starts at `line`; `space` is the last place it may
    // be broken, the last space or the start of the line.
    let mut line = 0;
    let mut space = 0;
    let mut cursor = 0;
    while cursor < string.len() {
        if string[cursor..].starts_with(line_break) {
            let end = cursor + line_break.len();
            result.extend_from_slice(&string[line..end]);
            cursor = end;
            line = end;
            space = end;
            continue;
        }
        let too_long = cursor.saturating_sub(line) as i64 >= width;
        if string[cursor] == b' ' {
            if too_long {
                result.extend_from_slice(&string[line..cursor]);
                result.extend_from_slice(line_break);
                line = cursor + 1;
            }
            space = cursor;
        } else if too_long && cut && line >= space {
            // No space in this line: cut the word here.
            result.extend_from_slice(&string[line..cursor]);
            result.extend_from_slice(line_break);
            line = cursor;
            space = cursor;
        } else if too_long && line < space {
            result.extend_from_slice(&string[line..space]);
            result.extend_from_slice(line_break);
            line = space + 1;
            space = line;
        }
        cursor += 1;
    }
    result.extend_from_slice(&string[line..]);
    Ok(Value::from(result))
}

/// `chunk_split(string $string, int $length = 76, string $separator =
/// "\r\n"): string`, the separator after every `$length` bytes and at the
/// end, even of an empty string.
///
/// ```php
/// chunk_split("abcde", 2, "|"); // "ab|cd|e|"
/// chunk_split("", 2, "|");      // "|"
/// ```
const CHUNK_SPLIT: FunctionEntry = FunctionEntry::sync(
    "chunk_split",
    &[
        Param::new("string", ParamType::String),
        Param::new("length", ParamType::Int).optional(),
        Param::new("separator", ParamType::String).optional(),
    ],
    chunk_split,
);

fn chunk_split(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
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
    let chunks = string.len().div_ceil(length as usize).max(1);
    let mut result = buffer(
        string
            .len()
            .saturating_add(separator.len().saturating_mul(chunks)),
    )?;
    if string.is_empty() {
        result.extend_from_slice(separator);
    }
    for chunk in string.chunks(length as usize) {
        result.extend_from_slice(chunk);
        result.extend_from_slice(separator);
    }
    Ok(Value::from(result))
}

/// `str_word_count(string $string, int $format = 0, ?string $characters =
/// null): array|int`: the number of words (format 0), the words (1), or the
/// words keyed by their byte offset (2).
///
/// A word is a run of letters, `'` and `-`, plus any `$characters` (which
/// may hold ranges, as in `trim`). An apostrophe or hyphen at the very
/// start, and a hyphen at the very end, do not count.
///
/// ```php
/// str_word_count("Hello fri3nd, you're looking good"); // 6: "fri" and "nd"
/// str_word_count("Hello fri3nd", 1, "0..9");           // ["Hello", "fri3nd"]
/// str_word_count("a bc", 2);                           // [0 => "a", 2 => "bc"]
/// ```
const STR_WORD_COUNT: FunctionEntry = FunctionEntry::sync(
    "str_word_count",
    &[
        Param::new("string", ParamType::String),
        Param::new("format", ParamType::Int).optional(),
        Param::new("characters", ParamType::String)
            .nullable()
            .optional(),
    ],
    str_word_count,
);

#[derive(Clone, Copy)]
enum WordFormat {
    Count,
    List,
    ByOffset,
}

fn str_word_count(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let format = match args::int(arguments, 1).unwrap_or_default() {
        0 => WordFormat::Count,
        1 => WordFormat::List,
        2 => WordFormat::ByOffset,
        _ => {
            return Err(NativeError::value_error(
                "str_word_count(): Argument #2 ($format) must be a valid format value",
            ));
        }
    };
    // An empty string returns before the character list is read, so a bad
    // range in it does not warn.
    let words = if string.is_empty() {
        Vec::new()
    } else {
        let extra = character_mask(context, "str_word_count", args::bytes(arguments, 2));
        words(string, &extra)
    };
    Ok(match format {
        WordFormat::Count => Value::Int(words.len() as i64),
        WordFormat::List => Value::from(list(words.into_iter().map(|(_, word)| Value::from(word)))),
        WordFormat::ByOffset => {
            let mut result = PhpArray::new();
            for (offset, word) in words {
                result.insert(ArrayKey::Int(offset as i64), Value::from(word));
            }
            Value::from(result)
        }
    })
}

/// The words of `string` and the offsets where they start.
fn words<'a>(string: &'a [u8], extra: &[bool; 256]) -> Vec<(usize, &'a [u8])> {
    let in_word =
        |byte: u8| alphabetic(byte) || matches!(byte, b'\'' | b'-') || extra[usize::from(byte)];
    let mut start = 0;
    let mut end = string.len();
    if matches!(string[0], b'\'' | b'-') && !extra[usize::from(string[0])] {
        start = 1;
    }
    if string[end - 1] == b'-' && !extra[usize::from(b'-')] {
        end -= 1;
    }
    let mut words = Vec::new();
    let mut cursor = start;
    while cursor < end {
        let word_start = cursor;
        while cursor < end && in_word(string[cursor]) {
            cursor += 1;
        }
        if cursor > word_start {
            words.push((word_start, &string[word_start..cursor]));
        }
        cursor += 1;
    }
    words
}

/// `count_chars(string $string, int $mode = 0): array|string`, how often
/// each byte occurs.
///
/// Modes 0 to 2 return an array from byte value to count: of all 256 bytes,
/// of those that occur, of those that do not. Modes 3 and 4 return the
/// bytes that occur, or do not, as a string. Both are in byte order.
///
/// ```php
/// count_chars("abba", 1); // [97 => 2, 98 => 2]
/// count_chars("abba", 3); // "ab"
/// ```
const COUNT_CHARS: FunctionEntry = FunctionEntry::sync(
    "count_chars",
    &[
        Param::new("string", ParamType::String),
        Param::new("mode", ParamType::Int).optional(),
    ],
    count_chars,
);

/// Which bytes `count_chars` reports.
#[derive(Clone, Copy)]
enum Bytes {
    All,
    Present,
    Absent,
}

fn count_chars(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (bytes, as_counts) = match args::int(arguments, 1).unwrap_or_default() {
        0 => (Bytes::All, true),
        1 => (Bytes::Present, true),
        2 => (Bytes::Absent, true),
        3 => (Bytes::Present, false),
        4 => (Bytes::Absent, false),
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
    let reported = (0..=255u8).filter(|&byte| match bytes {
        Bytes::All => true,
        Bytes::Present => counts[usize::from(byte)] != 0,
        Bytes::Absent => counts[usize::from(byte)] == 0,
    });
    if !as_counts {
        return Ok(Value::from(reported.collect::<Vec<u8>>()));
    }
    let mut result = PhpArray::new();
    for byte in reported {
        result.insert(
            ArrayKey::Int(i64::from(byte)),
            Value::Int(counts[usize::from(byte)]),
        );
    }
    Ok(Value::from(result))
}
