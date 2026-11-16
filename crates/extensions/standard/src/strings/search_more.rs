//! Reverse searches, byte comparisons, and bounded substring counting.
use super::{bytes, find, find_ignore_case};
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, SyncFn, Value,
    args,
};
const SEARCH: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    Param::new("offset", ParamType::Int).optional(),
];
const PART: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    Param::new("before_needle", ParamType::Bool).optional(),
];
const COMPARE: &[Param] = &[
    Param::new("string1", ParamType::String),
    Param::new("string2", ParamType::String),
];
const LIMITED: &[Param] = &[
    Param::new("string1", ParamType::String),
    Param::new("string2", ParamType::String),
    Param::new("length", ParamType::Int),
];
const COUNT: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    Param::new("offset", ParamType::Int).optional(),
    Param::new("length", ParamType::Int).nullable().optional(),
];
const SPAN: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("characters", ParamType::String),
    Param::new("offset", ParamType::Int).optional(),
    Param::new("length", ParamType::Int).nullable().optional(),
];
const SUBCOMPARE: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    Param::new("offset", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
    Param::new("case_insensitive", ParamType::Bool).optional(),
];
const PBRK: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("characters", ParamType::String),
];
pub(crate) fn register(registry: &mut Registry) {
    for (name, params, body) in [
        ("strrpos", SEARCH, strrpos as SyncFn),
        ("strripos", SEARCH, strripos),
        ("strstr", PART, strstr),
        ("strchr", PART, strstr),
        ("stristr", PART, stristr),
        ("strrchr", PART, strrchr),
        ("strpbrk", PBRK, strpbrk),
        ("strcmp", COMPARE, strcmp),
        ("strcasecmp", COMPARE, strcasecmp),
        ("strncmp", LIMITED, strncmp),
        ("strncasecmp", LIMITED, strncasecmp),
        ("substr_count", COUNT, substr_count),
        ("substr_compare", SUBCOMPARE, substr_compare),
        ("strspn", SPAN, strspn),
        ("strcspn", SPAN, strcspn),
    ] {
        registry.function(FunctionEntry::sync(name, params, body));
    }
}
/// Negative offsets cap the starting position of the last match, not its end.
fn reverse(arguments: &[Value], name: &str, fold: bool) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let needle = args::bytes(arguments, 1);
    let offset = args::int(arguments, 2).unwrap_or_default();
    if offset.unsigned_abs() > haystack.len() as u64 {
        return Err(offset_error(name));
    }
    let start = if offset < 0 { 0 } else { offset as usize };
    let Some(mut end) = haystack.len().checked_sub(needle.len()) else {
        return Ok(Value::Bool(false));
    };
    if offset < 0 {
        end = end.min(haystack.len() - offset.unsigned_abs() as usize);
    }
    let found = (start..=end).rev().find(|&index| {
        haystack
            .get(index..index + needle.len())
            .is_some_and(|part| {
                if fold {
                    part.eq_ignore_ascii_case(needle)
                } else {
                    part == needle
                }
            })
    });
    Ok(found.map_or(Value::Bool(false), |index| Value::Int(index as i64)))
}
fn offset_error(name: &str) -> NativeError {
    NativeError::value_error(format!(
        "{name}(): Argument #3 ($offset) must be contained in argument #1 ($haystack)"
    ))
}
fn strrpos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    reverse(arguments, "strrpos", false)
}
fn strripos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    reverse(arguments, "strripos", true)
}
/// strrchr uses only the first needle byte, with an empty needle meaning NUL.
fn part(arguments: &[Value], fold: bool, last: bool) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let needle = args::bytes(arguments, 1);
    let position = if last {
        let byte = needle.first().copied().unwrap_or(0);
        haystack.iter().rposition(|&item| item == byte)
    } else if fold {
        find_ignore_case(haystack, needle)
    } else {
        find(haystack, needle)
    };
    Ok(position.map_or(Value::Bool(false), |index| {
        bytes(
            if args::bool(arguments, 2).unwrap_or(false) {
                haystack.get(..index)
            } else {
                haystack.get(index..)
            }
            .unwrap_or_default()
            .to_vec(),
        )
    }))
}
fn strstr(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    part(arguments, false, false)
}
fn stristr(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    part(arguments, true, false)
}
fn strrchr(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    part(arguments, false, true)
}
fn strpbrk(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let mask = args::bytes(arguments, 1);
    if mask.is_empty() {
        return Err(NativeError::value_error(
            "strpbrk(): Argument #2 ($characters) must be a non-empty string",
        ));
    }
    Ok(haystack
        .iter()
        .position(|byte| mask.contains(byte))
        .map_or(Value::Bool(false), |index| {
            bytes(haystack.get(index..).unwrap_or_default().to_vec())
        }))
}
/// PHP returns the byte difference for unequal bytes, and arguments length sign otherwise.
fn compare(left: &[u8], right: &[u8], fold: bool) -> Value {
    for (&left, &right) in left.iter().zip(right) {
        let (left, right) = if fold {
            (left.to_ascii_lowercase(), right.to_ascii_lowercase())
        } else {
            (left, right)
        };
        if left != right {
            return Value::Int(i64::from(left) - i64::from(right));
        }
    }
    Value::Int(match left.len().cmp(&right.len()) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    })
}
fn strcmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(compare(
        args::bytes(arguments, 0),
        args::bytes(arguments, 1),
        false,
    ))
}
fn strcasecmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(compare(
        args::bytes(arguments, 0),
        args::bytes(arguments, 1),
        true,
    ))
}
fn limited(arguments: &[Value], name: &str, fold: bool) -> NativeResult {
    let length = args::int(arguments, 2).unwrap_or_default();
    if length < 0 {
        return Err(NativeError::value_error(format!(
            "{name}(): Argument #3 ($length) must be greater than or equal to 0"
        )));
    }
    let left = args::bytes(arguments, 0);
    let right = args::bytes(arguments, 1);
    Ok(compare(
        left.get(..left.len().min(length as usize))
            .unwrap_or_default(),
        right
            .get(..right.len().min(length as usize))
            .unwrap_or_default(),
        fold,
    ))
}
fn strncmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    limited(arguments, "strncmp", false)
}
fn strncasecmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    limited(arguments, "strncasecmp", true)
}
/// PHP clamps negative starts for spans but rejects them for substring counts.
fn start(length: usize, offset: i64) -> usize {
    if offset < 0 {
        length.saturating_sub(offset.unsigned_abs() as usize)
    } else {
        length.min(offset as usize)
    }
}
fn span(arguments: &[Value], included: bool) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mask = args::bytes(arguments, 1);
    let start = start(string.len(), args::int(arguments, 2).unwrap_or_default());
    let remaining = string.len() - start;
    let length = match args::int(arguments, 3) {
        None => remaining,
        Some(n) if n < 0 => remaining.saturating_sub(n.unsigned_abs() as usize),
        Some(n) => remaining.min(n as usize),
    };
    Ok(Value::Int(
        string
            .iter()
            .skip(start)
            .take(length)
            .take_while(|byte| mask.contains(byte) == included)
            .count() as i64,
    ))
}
fn strspn(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    span(arguments, true)
}
fn strcspn(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    span(arguments, false)
}
/// Count only non-overlapping matches and validate the needle before bounds.
fn substr_count(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let needle = args::bytes(arguments, 1);
    let offset = args::int(arguments, 2).unwrap_or_default();
    if needle.is_empty() {
        return Err(NativeError::value_error(
            "substr_count(): Argument #2 ($needle) must not be empty",
        ));
    }
    if offset.unsigned_abs() > haystack.len() as u64 {
        return Err(offset_error("substr_count"));
    }
    let start = start(haystack.len(), offset);
    let remaining = haystack.len() - start;
    let length = args::int(arguments, 3).unwrap_or(remaining as i64);
    if length.unsigned_abs() > remaining as u64 {
        return Err(NativeError::value_error(
            "substr_count(): Argument #4 ($length) must be contained in argument #1 ($haystack)",
        ));
    }
    let length = if length < 0 {
        remaining - length.unsigned_abs() as usize
    } else {
        length as usize
    };
    let mut tail = haystack.get(start..start + length).unwrap_or_default();
    let mut count = 0;
    while let Some(index) = find(tail, needle) {
        count += 1;
        tail = tail.get(index + needle.len()..).unwrap_or_default();
    }
    Ok(Value::Int(count))
}
/// A zero comparison length bypasses even an invalid offset.
fn substr_compare(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let left = args::bytes(arguments, 0);
    let right = args::bytes(arguments, 1);
    let offset = args::int(arguments, 2).unwrap_or_default();
    let length = args::int(arguments, 3);
    if length == Some(0) {
        return Ok(Value::Int(0));
    }
    if length.is_some_and(|n| n < 0) {
        return Err(NativeError::value_error(
            "substr_compare(): Argument #4 ($length) must be greater than or equal to 0",
        ));
    }
    if offset > left.len() as i64 {
        return Err(offset_error("substr_compare"));
    }
    let left = left.get(start(left.len(), offset)..).unwrap_or_default();
    let length = length.map_or(usize::MAX, |n| n as usize);
    Ok(compare(
        left.get(..left.len().min(length)).unwrap_or_default(),
        right.get(..right.len().min(length)).unwrap_or_default(),
        args::bool(arguments, 4).unwrap_or(false),
    ))
}
