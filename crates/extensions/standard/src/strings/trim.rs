//! Trimming: `trim("  hi\n")` is `"hi"`, `rtrim("1.500", "0")` is `"1.5"`.
//!
//! The list of characters to strip may contain ranges: `trim($s, "a..z")`.
//! [`character_mask`] reads such a list, for `addcslashes` and
//! `str_word_count` as well.

use phera_extension_api::{
    Change, Context, FunctionEntry, LanguageLevel, NativeFn, NativeResult, Param, ParamType,
    SyncFn, Value, args,
};

pub(super) fn functions(level: LanguageLevel) -> [FunctionEntry; 4] {
    let form_feed = level.has(Change::TrimFormFeed);
    [
        (TRIM, trim::<true> as SyncFn),
        (LTRIM, ltrim::<true> as SyncFn),
        (RTRIM, rtrim::<true> as SyncFn),
        (CHOP, chop::<true> as SyncFn),
    ]
    .map(|(entry, updated)| {
        let handler = if form_feed {
            NativeFn::Sync(updated)
        } else {
            entry.handler
        };
        FunctionEntry { handler, ..entry }
    })
}

/// What the trim functions strip by default: space, `\n`, `\r`, `\t`, `\v`
/// and NUL. PHP 8.6 also strips form feed.
const DEFAULT_CHARACTERS: &[u8] = b" \n\r\t\x0b\0";

/// The bytes a character list such as `"a..z\n"` names, as `php_charmask`
/// in ext/standard/string.c reads it.
///
/// `x..y` with `x <= y` is the range of bytes from `x` to `y`. Any other
/// `..` warns (PHP tries to say what is wrong with it) and then counts as
/// characters: `"z..a"` names `z`, `.` and `a`.
pub(super) fn character_mask(
    context: &mut dyn Context,
    function: &str,
    list: &[u8],
) -> [bool; 256] {
    let mut mask = [false; 256];
    let mut cursor = 0;
    while cursor < list.len() {
        let byte = list[cursor];
        if let &[_, b'.', b'.', last, ..] = &list[cursor..]
            && last >= byte
        {
            mask[usize::from(byte)..=usize::from(last)].fill(true);
            cursor += 4;
            continue;
        }
        if byte == b'.' && list.get(cursor + 1) == Some(&b'.') {
            // The first dot of a bad range is dropped; the second one is
            // read as a character on the next step.
            let reason = if cursor == 0 {
                ", no character to the left of '..'"
            } else if cursor + 2 >= list.len() {
                ", no character to the right of '..'"
            } else if list[cursor - 1] > list[cursor + 2] {
                ", '..'-range needs to be incrementing"
            } else {
                ""
            };
            context.warning(format!("{function}(): Invalid '..'-range{reason}"));
        } else {
            mask[usize::from(byte)] = true;
        }
        cursor += 1;
    }
    mask
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
    Both,
}

/// The string without the listed characters at the start, the end or both.
fn trim_with<const STRIP_FORM_FEED: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
    function: &str,
    side: Side,
) -> NativeResult {
    let characters = if args::given(arguments, 1) {
        args::bytes(arguments, 1)
    } else if STRIP_FORM_FEED {
        b" \n\r\t\x0b\x0c\0"
    } else {
        DEFAULT_CHARACTERS
    };
    let strip = character_mask(context, function, characters);
    let keep = |byte: &u8| !strip[usize::from(*byte)];
    let mut trimmed = args::bytes(arguments, 0);
    if side != Side::Right {
        let start = trimmed.iter().position(keep).unwrap_or(trimmed.len());
        trimmed = &trimmed[start..];
    }
    if side != Side::Left {
        let end = trimmed.iter().rposition(keep).map_or(0, |last| last + 1);
        trimmed = &trimmed[..end];
    }
    Ok(Value::from(trimmed))
}

/// `trim(string $string, string $characters = " \n\r\t\v\0"): string`
///
/// ```php
/// trim("  hello\n");       // "hello"
/// trim("[hello]", "[]");   // "hello"
/// trim("abcba", "a..b");   // "c"
/// ```
const TRIM: FunctionEntry = FunctionEntry::sync(
    "trim",
    &[
        Param::new("string", ParamType::String),
        Param::new("characters", ParamType::String).optional(),
    ],
    trim::<false>,
);

fn trim<const STRIP_FORM_FEED: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    trim_with::<STRIP_FORM_FEED>(context, arguments, "trim", Side::Both)
}

/// `ltrim(string $string, string $characters = " \n\r\t\v\0"): string`,
/// trimming the start only.
///
/// ```php
/// ltrim("0012", "0"); // "12"
/// ```
const LTRIM: FunctionEntry = FunctionEntry::sync(
    "ltrim",
    &[
        Param::new("string", ParamType::String),
        Param::new("characters", ParamType::String).optional(),
    ],
    ltrim::<false>,
);

fn ltrim<const STRIP_FORM_FEED: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    trim_with::<STRIP_FORM_FEED>(context, arguments, "ltrim", Side::Left)
}

/// `rtrim(string $string, string $characters = " \n\r\t\v\0"): string`,
/// trimming the end only.
///
/// ```php
/// rtrim("line\r\n"); // "line"
/// ```
const RTRIM: FunctionEntry = FunctionEntry::sync(
    "rtrim",
    &[
        Param::new("string", ParamType::String),
        Param::new("characters", ParamType::String).optional(),
    ],
    rtrim::<false>,
);

fn rtrim<const STRIP_FORM_FEED: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    trim_with::<STRIP_FORM_FEED>(context, arguments, "rtrim", Side::Right)
}

/// `chop()`, another name for `rtrim()`; its warnings name `chop()`.
const CHOP: FunctionEntry = FunctionEntry::sync(
    "chop",
    &[
        Param::new("string", ParamType::String),
        Param::new("characters", ParamType::String).optional(),
    ],
    chop::<false>,
);

fn chop<const STRIP_FORM_FEED: bool>(
    context: &mut dyn Context,
    arguments: &[Value],
) -> NativeResult {
    trim_with::<STRIP_FORM_FEED>(context, arguments, "chop", Side::Right)
}
