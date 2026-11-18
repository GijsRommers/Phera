//! Finding a needle: `strpos("hello", "l")` is 2, `substr_count("aaa", "a")`
//! is 3.
//!
//! An `$offset` may be negative to count from the end, but it must lie
//! inside the haystack: `strpos("abc", "a", 4)` throws a ValueError rather
//! than returning `false`. The functions with an `i` fold ASCII case only.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::{CaseSensitivity, find};
use crate::offsets::{clamped_range, contained_offset};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    STRPOS,
    STRIPOS,
    STRRPOS,
    STRRIPOS,
    STR_CONTAINS,
    STR_STARTS_WITH,
    STR_ENDS_WITH,
    SUBSTR_COUNT,
];

/// PHP's error for an `$offset` outside the haystack.
pub(super) fn offset_not_contained(function: &str) -> NativeError {
    NativeError::value_error(format!(
        "{function}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
    ))
}

/// A position as PHP's search functions return it: `false` for no match.
fn position_or_false(position: Option<usize>) -> Value {
    position.map_or(Value::Bool(false), |position| Value::Int(position as i64))
}

/// `strpos(string $haystack, string $needle, int $offset = 0): int|false`,
/// where the first `$needle` starts.
///
/// ```php
/// strpos("hello", "l");     // 2
/// strpos("hello", "l", -2); // 3: the search starts two bytes before the end
/// strpos("hello", "x");     // false
/// ```
const STRPOS: FunctionEntry = FunctionEntry::sync(
    "strpos",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
    ],
    strpos,
);

fn strpos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    find_first(arguments, "strpos", CaseSensitivity::Sensitive)
}

/// `stripos()`, `strpos()` ignoring ASCII case.
///
/// ```php
/// stripos("Hello", "L"); // 2
/// ```
const STRIPOS: FunctionEntry = FunctionEntry::sync(
    "stripos",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
    ],
    stripos,
);

fn stripos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    find_first(arguments, "stripos", CaseSensitivity::Insensitive)
}

fn find_first(arguments: &[Value], function: &str, case: CaseSensitivity) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let needle = args::bytes(arguments, 1);
    let offset = args::int(arguments, 2).unwrap_or_default();
    let start =
        contained_offset(haystack.len(), offset).ok_or_else(|| offset_not_contained(function))?;
    let found = case.find(&haystack[start..], needle);
    Ok(position_or_false(found.map(|position| start + position)))
}

/// `strrpos(string $haystack, string $needle, int $offset = 0): int|false`,
/// where the last `$needle` starts.
///
/// A positive offset is where the search begins. A negative one is the last
/// place a match may start, counted from the end.
///
/// ```php
/// strrpos("hello", "l");     // 3
/// strrpos("hello", "l", -3); // 2: no match may start after "hel"
/// ```
const STRRPOS: FunctionEntry = FunctionEntry::sync(
    "strrpos",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
    ],
    strrpos,
);

fn strrpos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    find_last(arguments, "strrpos", CaseSensitivity::Sensitive)
}

/// `strripos()`, `strrpos()` ignoring ASCII case.
///
/// ```php
/// strripos("HeLLo", "l"); // 3
/// ```
const STRRIPOS: FunctionEntry = FunctionEntry::sync(
    "strripos",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
    ],
    strripos,
);

fn strripos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    find_last(arguments, "strripos", CaseSensitivity::Insensitive)
}

fn find_last(arguments: &[Value], function: &str, case: CaseSensitivity) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let needle = args::bytes(arguments, 1);
    let offset = args::int(arguments, 2).unwrap_or_default();
    let position =
        contained_offset(haystack.len(), offset).ok_or_else(|| offset_not_contained(function))?;
    let Some(mut last) = haystack.len().checked_sub(needle.len()) else {
        return Ok(Value::Bool(false));
    };
    let mut first = 0;
    if offset < 0 {
        last = last.min(position);
    } else {
        first = position;
    }
    let found = (first..=last)
        .rev()
        .find(|&start| case.equal(&haystack[start..start + needle.len()], needle));
    Ok(position_or_false(found))
}

/// `str_contains(string $haystack, string $needle): bool`
///
/// ```php
/// str_contains("hello", "ell"); // true
/// str_contains("hello", "");    // true
/// ```
const STR_CONTAINS: FunctionEntry = FunctionEntry::sync(
    "str_contains",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
    ],
    str_contains,
);

fn str_contains(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let found = find(args::bytes(arguments, 0), args::bytes(arguments, 1));
    Ok(Value::Bool(found.is_some()))
}

/// `str_starts_with(string $haystack, string $needle): bool`
///
/// ```php
/// str_starts_with("hello", "he"); // true
/// ```
const STR_STARTS_WITH: FunctionEntry = FunctionEntry::sync(
    "str_starts_with",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
    ],
    str_starts_with,
);

fn str_starts_with(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    Ok(Value::Bool(haystack.starts_with(args::bytes(arguments, 1))))
}

/// `str_ends_with(string $haystack, string $needle): bool`
///
/// ```php
/// str_ends_with("hello", "lo"); // true
/// ```
const STR_ENDS_WITH: FunctionEntry = FunctionEntry::sync(
    "str_ends_with",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
    ],
    str_ends_with,
);

fn str_ends_with(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    Ok(Value::Bool(haystack.ends_with(args::bytes(arguments, 1))))
}

/// `substr_count(string $haystack, string $needle, int $offset = 0,
/// ?int $length = null): int`, how often `$needle` occurs without
/// overlapping, in the part that `$offset` and `$length` select.
///
/// Unlike `substr`, both must lie inside the haystack, and the needle is
/// checked first.
///
/// ```php
/// substr_count("aaaa", "aa");      // 2
/// substr_count("hello", "l", -2);  // 1
/// substr_count("hello", "l", 0, 9); // ValueError
/// ```
const SUBSTR_COUNT: FunctionEntry = FunctionEntry::sync(
    "substr_count",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
        Param::new("offset", ParamType::Int).optional(),
        Param::new("length", ParamType::Int).nullable().optional(),
    ],
    substr_count,
);

fn substr_count(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let needle = args::bytes(arguments, 1);
    if needle.is_empty() {
        return Err(NativeError::value_error(
            "substr_count(): Argument #2 ($needle) must not be empty",
        ));
    }
    let offset = args::int(arguments, 2).unwrap_or_default();
    let start = contained_offset(haystack.len(), offset)
        .ok_or_else(|| offset_not_contained("substr_count"))?;
    let length = args::int(arguments, 3);
    if length.is_some_and(|length| length.unsigned_abs() > (haystack.len() - start) as u64) {
        return Err(NativeError::value_error(
            "substr_count(): Argument #4 ($length) must be contained in argument #1 ($haystack)",
        ));
    }
    let mut rest = &haystack[clamped_range(haystack.len(), offset, length)];
    let mut count = 0;
    while let Some(position) = find(rest, needle) {
        count += 1;
        rest = &rest[position + needle.len()..];
    }
    Ok(Value::Int(count))
}
