//! Byte comparisons: `strcmp("a", "c")` is -2, `strcasecmp("A", "a")` is 0.
//!
//! Like C's `memcmp`, PHP returns the difference of the first two bytes that
//! differ. When one string is a prefix of the other, it returns -1 or 1
//! (since PHP 8.2; before, the difference of the lengths): `strcmp("ab",
//! "abcd")` is -1. This is `zend_binary_strcmp` in Zend/zend_operators.c.

use std::cmp::Ordering;

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::CaseSensitivity;
use super::search::offset_not_contained;

pub(super) const FUNCTIONS: &[FunctionEntry] =
    &[STRCMP, STRCASECMP, STRNCMP, STRNCASECMP, SUBSTR_COMPARE];

fn compare(left: &[u8], right: &[u8], case: CaseSensitivity) -> Value {
    for (&left, &right) in left.iter().zip(right) {
        let (left, right) = match case {
            CaseSensitivity::Sensitive => (left, right),
            CaseSensitivity::Insensitive => (left.to_ascii_lowercase(), right.to_ascii_lowercase()),
        };
        if left != right {
            return Value::Int(i64::from(left) - i64::from(right));
        }
    }
    Value::Int(match left.len().cmp(&right.len()) {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    })
}

/// At most the first `length` bytes.
fn prefix(string: &[u8], length: usize) -> &[u8] {
    &string[..string.len().min(length)]
}

/// `strcmp(string $string1, string $string2): int`
///
/// ```php
/// strcmp("a", "c");     // -2
/// strcmp("ab", "abcd"); // -1
/// ```
const STRCMP: FunctionEntry = FunctionEntry::sync(
    "strcmp",
    &[
        Param::new("string1", ParamType::String),
        Param::new("string2", ParamType::String),
    ],
    strcmp,
);

fn strcmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (left, right) = (args::bytes(arguments, 0), args::bytes(arguments, 1));
    Ok(compare(left, right, CaseSensitivity::Sensitive))
}

/// `strcasecmp()`, `strcmp()` ignoring ASCII case.
///
/// ```php
/// strcasecmp("HELLO", "hello"); // 0
/// ```
const STRCASECMP: FunctionEntry = FunctionEntry::sync(
    "strcasecmp",
    &[
        Param::new("string1", ParamType::String),
        Param::new("string2", ParamType::String),
    ],
    strcasecmp,
);

fn strcasecmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (left, right) = (args::bytes(arguments, 0), args::bytes(arguments, 1));
    Ok(compare(left, right, CaseSensitivity::Insensitive))
}

/// `strncmp(string $string1, string $string2, int $length): int`, comparing
/// the first `$length` bytes only.
///
/// ```php
/// strncmp("apple", "apricot", 2); // 0
/// strncmp("a", "b", -1);          // ValueError
/// ```
const STRNCMP: FunctionEntry = FunctionEntry::sync(
    "strncmp",
    &[
        Param::new("string1", ParamType::String),
        Param::new("string2", ParamType::String),
        Param::new("length", ParamType::Int),
    ],
    strncmp,
);

fn strncmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    compare_prefixes(arguments, "strncmp", CaseSensitivity::Sensitive)
}

/// `strncasecmp()`, `strncmp()` ignoring ASCII case.
///
/// ```php
/// strncasecmp("Hello", "help", 3); // 0
/// ```
const STRNCASECMP: FunctionEntry = FunctionEntry::sync(
    "strncasecmp",
    &[
        Param::new("string1", ParamType::String),
        Param::new("string2", ParamType::String),
        Param::new("length", ParamType::Int),
    ],
    strncasecmp,
);

fn strncasecmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    compare_prefixes(arguments, "strncasecmp", CaseSensitivity::Insensitive)
}

fn compare_prefixes(arguments: &[Value], function: &str, case: CaseSensitivity) -> NativeResult {
    let length = args::int(arguments, 2).unwrap_or_default();
    let Ok(length) = usize::try_from(length) else {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #3 ($length) must be greater than or equal to 0"
        )));
    };
    let left = prefix(args::bytes(arguments, 0), length);
    let right = prefix(args::bytes(arguments, 1), length);
    Ok(compare(left, right, case))
}

/// `substr_compare(string $haystack, string $needle, int $offset,
/// ?int $length = null, bool $case_insensitive = false): int`, comparing the
/// haystack from `$offset` on with the needle, up to `$length` bytes.
///
/// A length of 0 is 0 before anything else is checked. Otherwise the offset
/// may not lie past the end, but a negative one too far back starts at 0.
///
/// ```php
/// substr_compare("Hello", "ello", 1);    // 0
/// substr_compare("Hello", "LL", -3, 2, true); // 0
/// ```
const SUBSTR_COMPARE: FunctionEntry = FunctionEntry::sync(
    "substr_compare",
    &[
        Param::new("haystack", ParamType::String),
        Param::new("needle", ParamType::String),
        Param::new("offset", ParamType::Int),
        Param::new("length", ParamType::Int).nullable().optional(),
        Param::new("case_insensitive", ParamType::Bool).optional(),
    ],
    substr_compare,
);

fn substr_compare(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let needle = args::bytes(arguments, 1);
    let offset = args::int(arguments, 2).unwrap_or_default();
    let length = match args::int(arguments, 3) {
        Some(0) => return Ok(Value::Int(0)),
        Some(length) => usize::try_from(length).map_err(|_| {
            NativeError::value_error(
                "substr_compare(): Argument #4 ($length) must be greater than or equal to 0",
            )
        })?,
        None => usize::MAX,
    };
    if offset > haystack.len() as i64 {
        return Err(offset_not_contained("substr_compare"));
    }
    let start = if offset < 0 {
        haystack
            .len()
            .saturating_sub(offset.unsigned_abs() as usize)
    } else {
        offset as usize
    };
    let case = if args::bool(arguments, 4).unwrap_or(false) {
        CaseSensitivity::Insensitive
    } else {
        CaseSensitivity::Sensitive
    };
    Ok(compare(
        prefix(&haystack[start..], length),
        prefix(needle, length),
        case,
    ))
}
