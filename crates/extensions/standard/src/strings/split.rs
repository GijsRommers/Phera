//! Splitting and joining: `explode(",", "a,b")` is `["a", "b"]`,
//! `implode("-", ["a", "b"])` is `"a-b"`.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Value, args,
};

use super::{buffer, find, reserve};
use crate::arrays::list;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[IMPLODE, JOIN, EXPLODE, STR_REPEAT, STR_SPLIT];

/// `implode(string $separator, array $array): string`, the values of the
/// array as strings, with the separator between them.
///
/// The one-argument form `implode($array)` joins without a separator. The
/// separator-last form of PHP 7 is a TypeError since PHP 8.
///
/// ```php
/// implode(", ", ["a", 1, null]); // "a, 1, "
/// implode(["a", "b"]);           // "ab"
/// ```
const IMPLODE: FunctionEntry = FunctionEntry::asynchronous(
    "implode",
    &[
        Param::new("separator", ParamType::ArrayOrString),
        Param::new("array", ParamType::Array).nullable().optional(),
    ],
    implode,
);

fn implode(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(join_values(context, arguments, "implode"))
}

/// `join()`, another name for `implode()`; its errors name `join()`.
const JOIN: FunctionEntry = FunctionEntry::asynchronous(
    "join",
    &[
        Param::new("separator", ParamType::ArrayOrString),
        Param::new("array", ParamType::Array).nullable().optional(),
    ],
    join,
);

fn join(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(join_values(context, arguments, "join"))
}

async fn join_values(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let arguments = &arguments;
    let (separator, array) = match (args::array(arguments, 0), args::array(arguments, 1)) {
        (None, Some(array)) => (args::bytes(arguments, 0), array),
        (Some(array), None) => (&b""[..], array),
        (Some(_), Some(_)) => {
            return Err(NativeError::type_error(format!(
                "{function}(): Argument #1 ($separator) must be of type string, array given"
            )));
        }
        (None, None) => {
            return Err(NativeError::type_error(format!(
                "{function}(): If argument #1 ($separator) is of type string, \
                 argument #2 ($array) must be of type array, null given"
            )));
        }
    };
    let mut result = Vec::new();
    for (index, value) in array.values().enumerate() {
        let string = context.stringify(value).await?;
        let separator = if index > 0 { separator } else { &[] };
        reserve(&mut result, separator.len() + string.len())?;
        result.extend_from_slice(separator);
        result.extend_from_slice(string.as_bytes());
    }
    Ok(Value::from(result))
}

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
    // An empty string repeated PHP_INT_MAX times is empty at once.
    if !string.is_empty() {
        for _ in 0..times {
            result.extend_from_slice(string);
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
