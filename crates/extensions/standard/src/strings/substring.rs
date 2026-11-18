//! Parts of a string: `substr("hello", 1, 3)` is `"ell"`, `strstr("a@b",
//! "@")` is `"@b"`, `strspn("42 apples", "0123456789")` is 2.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::CaseSensitivity;
use crate::offsets::clamped_range;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    SUBSTR, STRSTR, STRCHR, STRISTR, STRRCHR, STRPBRK, STRSPN, STRCSPN,
];

/// `substr(string $string, int $offset, ?int $length = null): string`
///
/// Offset and length are clamped to the string, so `substr` never fails.
///
/// ```php
/// substr("hello", 1, 3);  // "ell"
/// substr("hello", -3);    // "llo"
/// substr("hello", 1, -1); // "ell"
/// substr("hello", 9);     // ""
/// ```
const SUBSTR: FunctionEntry = FunctionEntry::sync(
    "substr",
    &[
        Param::new("string", ParamType::String),
        Param::new("offset", ParamType::Int),
        Param::new("length", ParamType::Int).nullable().optional(),
    ],
    substr,
);

fn substr(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let offset = args::int(arguments, 1).unwrap_or_default();
    let range = clamped_range(string.len(), offset, args::int(arguments, 2));
    Ok(Value::from(&string[range]))
}

/// `strstr(string $haystack, string $needle, bool $before_needle = false):
/// string|false`, the haystack from the first `$needle` on, or the part
/// before it.
///
/// ```php
/// strstr("user@example.com", "@");       // "@example.com"
/// strstr("user@example.com", "@", true); // "user"
/// ```
const STRSTR: FunctionEntry = FunctionEntry::sync(
    "strstr",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
        Param::new("before_needle", ParamType::Bool).optional(),
    ],
    strstr,
);

fn strstr(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let found = CaseSensitivity::Sensitive.find(haystack, args::bytes(arguments, 1));
    Ok(split_at_match(arguments, haystack, found))
}

/// `strchr()`, another name for `strstr()`.
const STRCHR: FunctionEntry = FunctionEntry::sync(
    "strchr",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
        Param::new("before_needle", ParamType::Bool).optional(),
    ],
    strstr,
);

/// `stristr()`, `strstr()` ignoring ASCII case.
///
/// ```php
/// stristr("Hello World", "WORLD"); // "World"
/// ```
const STRISTR: FunctionEntry = FunctionEntry::sync(
    "stristr",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
        Param::new("before_needle", ParamType::Bool).optional(),
    ],
    stristr,
);

fn stristr(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let found = CaseSensitivity::Insensitive.find(haystack, args::bytes(arguments, 1));
    Ok(split_at_match(arguments, haystack, found))
}

/// `strrchr(string $haystack, string $needle, bool $before_needle = false):
/// string|false`, the haystack from the last occurrence of a byte on.
///
/// Only the needle's first byte counts, and an empty needle stands for NUL.
///
/// ```php
/// strrchr("a/b/c", "/");   // "/c"
/// strrchr("a/b/c", "/x");  // "/c" as well
/// ```
const STRRCHR: FunctionEntry = FunctionEntry::sync(
    "strrchr",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
        Param::new("before_needle", ParamType::Bool).optional(),
    ],
    strrchr,
);

fn strrchr(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let byte = args::bytes(arguments, 1).first().copied().unwrap_or(0);
    let found = haystack.iter().rposition(|&item| item == byte);
    Ok(split_at_match(arguments, haystack, found))
}

/// What the `strstr` family returns for a match at `position`: the rest of
/// the haystack, or the part before it when `$before_needle` is true.
fn split_at_match(arguments: &[Value], haystack: &[u8], position: Option<usize>) -> Value {
    let Some(position) = position else {
        return Value::Bool(false);
    };
    if args::bool(arguments, 2).unwrap_or(false) {
        Value::from(&haystack[..position])
    } else {
        Value::from(&haystack[position..])
    }
}

/// `strpbrk(string $string, string $characters): string|false`, the string
/// from the first of the characters on.
///
/// ```php
/// strpbrk("This is a test", "st"); // "s is a test"
/// strpbrk("abc", "");              // ValueError
/// ```
const STRPBRK: FunctionEntry = FunctionEntry::sync(
    "strpbrk",
    &[
        Param::new("string", ParamType::String),
        Param::new("characters", ParamType::String),
    ],
    strpbrk,
);

fn strpbrk(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let characters = args::bytes(arguments, 1);
    if characters.is_empty() {
        return Err(NativeError::value_error(
            "strpbrk(): Argument #2 ($characters) must be a non-empty string",
        ));
    }
    Ok(
        match string.iter().position(|byte| characters.contains(byte)) {
            Some(position) => Value::from(&string[position..]),
            None => Value::Bool(false),
        },
    )
}

/// `strspn(string $string, string $characters, int $offset = 0,
/// ?int $length = null): int`, how many bytes at the start consist of the
/// characters only. Offset and length are clamped as in `substr`.
///
/// ```php
/// strspn("42 apples", "0123456789"); // 2
/// strspn("foo", "o", 1, 2);          // 2
/// ```
const STRSPN: FunctionEntry = FunctionEntry::sync(
    "strspn",
    &[
        Param::new("string", ParamType::String),
        Param::new("characters", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("length", ParamType::Int).nullable().optional(),
    ],
    strspn,
);

fn strspn(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(span(arguments, Span::Inside)))
}

/// `strcspn()`, how many bytes at the start are none of the characters.
///
/// ```php
/// strcspn("abcd", "cd"); // 2
/// ```
const STRCSPN: FunctionEntry = FunctionEntry::sync(
    "strcspn",
    &[
        Param::new("string", ParamType::String),
        Param::new("characters", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("length", ParamType::Int).nullable().optional(),
    ],
    strcspn,
);

fn strcspn(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(Value::Int(span(arguments, Span::Outside)))
}

/// Whether a span runs over bytes that are in the character list or not.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Span {
    Inside,
    Outside,
}

fn span(arguments: &[Value], span: Span) -> i64 {
    let string = args::bytes(arguments, 0);
    let characters = args::bytes(arguments, 1);
    let offset = args::int(arguments, 2).unwrap_or_default();
    let range = clamped_range(string.len(), offset, args::int(arguments, 3));
    let in_span = |byte: &&u8| characters.contains(byte) == (span == Span::Inside);
    string[range].iter().take_while(in_span).count() as i64
}
