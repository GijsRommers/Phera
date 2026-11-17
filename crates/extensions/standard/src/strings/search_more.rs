//! Reverse searches, byte comparisons, and bounded substring counting.

use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, SyncFn, Value,
    args,
};

use super::{CaseSensitivity, bytes, find, find_ignore_case};

enum PartSearch {
    First(CaseSensitivity),
    LastByte,
}

enum Membership {
    Included,
    Excluded,
}

mod comparison;

use comparison::{strcasecmp, strcmp, strncasecmp, strncmp, substr_compare};

const SEARCH_PARAMS: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    Param::new("offset", ParamType::Int).optional(),
];

const PART_PARAMS: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    Param::new("before_needle", ParamType::Bool).optional(),
];

const COMPARE_PARAMS: &[Param] = &[
    Param::new("string1", ParamType::String),
    Param::new("string2", ParamType::String),
];

const LIMITED_PARAMS: &[Param] = &[
    Param::new("string1", ParamType::String),
    Param::new("string2", ParamType::String),
    Param::new("length", ParamType::Int),
];

const COUNT_PARAMS: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    Param::new("offset", ParamType::Int).optional(),
    Param::new("length", ParamType::Int).nullable().optional(),
];

const SPAN_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("characters", ParamType::String),
    Param::new("offset", ParamType::Int).optional(),
    Param::new("length", ParamType::Int).nullable().optional(),
];

const SUBCOMPARE_PARAMS: &[Param] = &[
    Param::new("haystack", ParamType::String),
    Param::new("needle", ParamType::String),
    Param::new("offset", ParamType::Int),
    Param::new("length", ParamType::Int).nullable().optional(),
    Param::new("case_insensitive", ParamType::Bool).optional(),
];

const PBRK_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("characters", ParamType::String),
];

pub(crate) fn register(registry: &mut Registry) {
    for (name, params, body) in [
        ("strrpos", SEARCH_PARAMS, strrpos as SyncFn),
        ("strripos", SEARCH_PARAMS, strripos),
        ("strstr", PART_PARAMS, strstr),
        ("strchr", PART_PARAMS, strstr),
        ("stristr", PART_PARAMS, stristr),
        ("strrchr", PART_PARAMS, strrchr),
        ("strpbrk", PBRK_PARAMS, strpbrk),
        ("strcmp", COMPARE_PARAMS, strcmp),
        ("strcasecmp", COMPARE_PARAMS, strcasecmp),
        ("strncmp", LIMITED_PARAMS, strncmp),
        ("strncasecmp", LIMITED_PARAMS, strncasecmp),
        ("substr_count", COUNT_PARAMS, substr_count),
        ("substr_compare", SUBCOMPARE_PARAMS, substr_compare),
        ("strspn", SPAN_PARAMS, strspn),
        ("strcspn", SPAN_PARAMS, strcspn),
    ] {
        registry.function(FunctionEntry::sync(name, params, body));
    }
}

/// Negative offsets cap the starting position of the last match, not its end.
fn find_last(arguments: &[Value], name: &str, sensitivity: CaseSensitivity) -> NativeResult {
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
                if sensitivity == CaseSensitivity::Insensitive {
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
    find_last(arguments, "strrpos", CaseSensitivity::Sensitive)
}

fn strripos(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    find_last(arguments, "strripos", CaseSensitivity::Insensitive)
}

/// strrchr uses only the first needle byte, with an empty needle meaning NUL.
fn part(arguments: &[Value], mode: PartSearch) -> NativeResult {
    let haystack = args::bytes(arguments, 0);
    let needle = args::bytes(arguments, 1);
    let position = if matches!(mode, PartSearch::LastByte) {
        let byte = needle.first().copied().unwrap_or(0);
        haystack.iter().rposition(|&item| item == byte)
    } else if matches!(mode, PartSearch::First(CaseSensitivity::Insensitive)) {
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
    part(arguments, PartSearch::First(CaseSensitivity::Sensitive))
}

fn stristr(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    part(arguments, PartSearch::First(CaseSensitivity::Insensitive))
}

fn strrchr(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    part(arguments, PartSearch::LastByte)
}

/// An empty character mask is an error rather than an unsuccessful match.
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

/// PHP clamps negative starts for spans but rejects them for substring counts.
fn start(length: usize, offset: i64) -> usize {
    if offset < 0 {
        length.saturating_sub(offset.unsigned_abs() as usize)
    } else {
        length.min(offset as usize)
    }
}

/// Span lengths clamp to the remaining bytes, including negative lengths.
fn span(arguments: &[Value], membership: Membership) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let mask = args::bytes(arguments, 1);
    let start = start(string.len(), args::int(arguments, 2).unwrap_or_default());
    let remaining = string.len() - start;
    let length = match args::int(arguments, 3) {
        None => remaining,
        Some(count) if count < 0 => remaining.saturating_sub(count.unsigned_abs() as usize),
        Some(count) => remaining.min(count as usize),
    };
    Ok(Value::Int(
        string
            .iter()
            .skip(start)
            .take(length)
            .take_while(|byte| mask.contains(byte) == matches!(membership, Membership::Included))
            .count() as i64,
    ))
}

fn strspn(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    span(arguments, Membership::Included)
}

fn strcspn(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    span(arguments, Membership::Excluded)
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
