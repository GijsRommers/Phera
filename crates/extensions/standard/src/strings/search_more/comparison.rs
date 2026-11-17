//! Byte comparisons with PHP length and offset validation.

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use super::super::CaseSensitivity;
use super::{offset_error, start};

/// PHP returns the byte difference for unequal bytes, and arguments length sign otherwise.
fn compare(left: &[u8], right: &[u8], sensitivity: CaseSensitivity) -> Value {
    for (&left, &right) in left.iter().zip(right) {
        let (left, right) = if sensitivity == CaseSensitivity::Insensitive {
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

pub(super) fn strcmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(compare(
        args::bytes(arguments, 0),
        args::bytes(arguments, 1),
        CaseSensitivity::Sensitive,
    ))
}

pub(super) fn strcasecmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    Ok(compare(
        args::bytes(arguments, 0),
        args::bytes(arguments, 1),
        CaseSensitivity::Insensitive,
    ))
}

/// PHP rejects negative limits before reading either comparison prefix.
fn limited(arguments: &[Value], name: &str, sensitivity: CaseSensitivity) -> NativeResult {
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
        sensitivity,
    ))
}

pub(super) fn strncmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    limited(arguments, "strncmp", CaseSensitivity::Sensitive)
}

pub(super) fn strncasecmp(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    limited(arguments, "strncasecmp", CaseSensitivity::Insensitive)
}

/// A zero comparison length bypasses even an invalid offset.
pub(super) fn substr_compare(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let left = args::bytes(arguments, 0);
    let right = args::bytes(arguments, 1);
    let offset = args::int(arguments, 2).unwrap_or_default();
    let length = args::int(arguments, 3);
    if length == Some(0) {
        return Ok(Value::Int(0));
    }
    if length.is_some_and(|count| count < 0) {
        return Err(NativeError::value_error(
            "substr_compare(): Argument #4 ($length) must be greater than or equal to 0",
        ));
    }
    if offset > left.len() as i64 {
        return Err(offset_error("substr_compare"));
    }
    let left = left.get(start(left.len(), offset)..).unwrap_or_default();
    let length = length.map_or(usize::MAX, |count| count as usize);
    Ok(compare(
        left.get(..left.len().min(length)).unwrap_or_default(),
        right.get(..right.len().min(length)).unwrap_or_default(),
        if args::bool(arguments, 4).unwrap_or(false) {
            CaseSensitivity::Insensitive
        } else {
            CaseSensitivity::Sensitive
        },
    ))
}
