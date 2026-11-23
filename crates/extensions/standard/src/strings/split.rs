//! Splitting strings: `explode(",", "a,b")` is `["a", "b"]`.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::{buffer, find, join};
use crate::arrays::list;

pub(super) const FUNCTIONS: &[FunctionEntry] =
    &[join::IMPLODE, join::JOIN, EXPLODE, STR_REPEAT, STR_SPLIT];

/// `explode(string $separator, string $string, int $limit = PHP_INT_MAX):
/// array`, the parts between the separators.
///
/// A positive limit caps the number of parts, the last one holding the
/// rest; 0 counts as 1. A negative limit drops that many parts at the end.
///
/// ```php
/// explode(",", "a,b,c");     // ["a", "b", "c"]
/// explode(",", "a,b,c", 2);  // ["a", "b,c"]
/// explode(",", "a,b,c", -1); // ["a", "b"]
/// ```
const EXPLODE: FunctionEntry = FunctionEntry::sync(
    "explode",
    &[
        Param::new("separator", ParamType::String),
        Param::new("string", ParamType::String),
        Param::new("limit", ParamType::Int).optional(),
    ],
    explode,
);

fn explode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let separator = args::bytes(arguments, 0);
    let string = args::bytes(arguments, 1);
    let limit = args::int(arguments, 2).unwrap_or(i64::MAX);
    if separator.is_empty() {
        return Err(NativeError::value_error(
            "explode(): Argument #1 ($separator) must not be empty",
        ));
    }
    let most_parts = if limit < 0 {
        usize::MAX
    } else {
        limit.max(1) as usize
    };
    let mut remaining = most_parts;
    let mut rest = Some(string);
    let parts = std::iter::from_fn(|| {
        let part = rest.take()?;
        if remaining > 1
            && let Some(position) = find(part, separator)
        {
            remaining -= 1;
            rest = Some(&part[position + separator.len()..]);
            return Some(&part[..position]);
        }
        Some(part)
    });
    if limit < 0 {
        // The discarded suffix must not allocate PHP strings.
        let mut retained: Vec<_> = parts.collect();
        retained.truncate(retained.len().saturating_sub(limit.unsigned_abs() as usize));
        return Ok(Value::from(list(retained.into_iter().map(Value::from))));
    }
    Ok(Value::from(list(parts.map(Value::from))))
}

/// `str_repeat(string $string, int $times): string`
///
/// ```php
/// str_repeat("ab", 3); // "ababab"
/// ```
const STR_REPEAT: FunctionEntry = FunctionEntry::sync(
    "str_repeat",
    &[
        Param::new("string", ParamType::String),
        Param::new("times", ParamType::Int),
    ],
    str_repeat,
);

fn str_repeat(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let Ok(times) = usize::try_from(args::int(arguments, 1).unwrap_or_default()) else {
        return Err(NativeError::value_error(
            "str_repeat(): Argument #2 ($times) must be greater than or equal to 0",
        ));
    };
    let length = string
        .len()
        .checked_mul(times)
        .ok_or_else(|| NativeError::error("String size exceeds the maximum allowed size"))?;
    let mut result = buffer(length)?;
    match string {
        // An empty string repeated PHP_INT_MAX times is empty at once.
        [] => {}
        [byte] => result.resize(length, *byte),
        _ => {
            for _ in 0..times {
                result.extend_from_slice(string);
            }
        }
    }
    Ok(Value::from(result))
}

/// `str_split(string $string, int $length = 1): array`, the string in
/// pieces of `$length` bytes; the last one may be shorter.
///
/// ```php
/// str_split("abcde", 2); // ["ab", "cd", "e"]
/// str_split("");         // [] since PHP 8.2
/// ```
const STR_SPLIT: FunctionEntry = FunctionEntry::sync(
    "str_split",
    &[
        Param::new("string", ParamType::String),
        Param::new("length", ParamType::Int).optional(),
    ],
    str_split,
);

fn str_split(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let length = args::int(arguments, 1).unwrap_or(1);
    if length < 1 {
        return Err(NativeError::value_error(
            "str_split(): Argument #2 ($length) must be greater than 0",
        ));
    }
    let pieces = args::bytes(arguments, 0).chunks(length as usize);
    Ok(Value::from(list(pieces.map(Value::from))))
}
