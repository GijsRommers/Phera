//! `version_compare('8.5.12RC1', '8.5.12', '<')` orders PHP's version components.

use std::cmp::Ordering;

use phera_extension_api::Value;
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, args,
};

pub(crate) const ENTRY: FunctionEntry = FunctionEntry::sync(
    "version_compare",
    &[
        Param::new("version1", ParamType::String),
        Param::new("version2", ParamType::String),
        Param::new("operator", ParamType::String)
            .optional()
            .nullable(),
    ],
    version_compare,
);

fn version_compare(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let left = args::string(arguments, 0);
    let right = args::string(arguments, 1);
    let comparison = compare(
        left.map_or(b"".as_slice(), |value| value.as_bytes()),
        right.map_or(b"".as_slice(), |value| value.as_bytes()),
    );
    let Some(Value::String(operator)) = arguments.get(2) else {
        return Ok(Value::Int(i64::from(comparison as i8)));
    };
    Ok(Value::Bool(match operator.as_bytes() {
        b"<" | b"lt" => comparison.is_lt(),
        b"<=" | b"le" => comparison.is_le(),
        b">" | b"gt" => comparison.is_gt(),
        b">=" | b"ge" => comparison.is_ge(),
        b"=" | b"==" | b"eq" => comparison.is_eq(),
        b"!=" | b"<>" | b"ne" => comparison.is_ne(),
        _ => {
            return Err(NativeError::value_error(
                "version_compare(): Argument #3 ($operator) must be a valid comparison operator",
            ));
        }
    }))
}

fn compare(left: &[u8], right: &[u8]) -> Ordering {
    let left = left.split(|byte| *byte == 0).next().unwrap_or_default();
    let right = right.split(|byte| *byte == 0).next().unwrap_or_default();
    if left.is_empty() || right.is_empty() {
        return right.is_empty().cmp(&left.is_empty());
    }
    let left = canonical(left);
    let right = canonical(right);
    let mut left = left.split(|byte| *byte == b'.').peekable();
    let mut right = right.split(|byte| *byte == b'.').peekable();
    while let (Some(l), Some(r)) = (left.peek(), right.peek()) {
        let order = compare_component(l, r);
        if !order.is_eq() {
            return order;
        }
        left.next();
        right.next();
    }
    match (left.next(), right.next()) {
        (Some(extra), None) => compare_extra(extra),
        (None, Some(extra)) => compare_extra(extra).reverse(),
        _ => Ordering::Equal,
    }
}

fn canonical(version: &[u8]) -> Vec<u8> {
    if version.starts_with(b"#") {
        return version.to_vec();
    }
    let mut previous = version[0];
    let mut result = vec![previous];
    for &byte in &version[1..] {
        let boundary =
            previous != b'.' && byte != b'.' && previous.is_ascii_digit() != byte.is_ascii_digit();
        if (boundary || !byte.is_ascii_alphanumeric()) && result.last() != Some(&b'.') {
            result.push(b'.');
        }
        if byte.is_ascii_alphanumeric() || (boundary && !matches!(byte, b'-' | b'_' | b'+')) {
            result.push(byte);
        }
        previous = byte;
    }
    if result.last() == Some(&b'.') {
        result.pop();
    }
    result
}

fn compare_component(left: &[u8], right: &[u8]) -> Ordering {
    match (number(left), number(right)) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => 4.cmp(&qualifier(right)),
        (None, Some(_)) => qualifier(left).cmp(&4),
        (None, None) => qualifier(left).cmp(&qualifier(right)),
    }
}

fn compare_extra(component: &[u8]) -> Ordering {
    if number(component).is_some() {
        Ordering::Greater
    } else {
        qualifier(component).cmp(&4)
    }
}

fn number(component: &[u8]) -> Option<i64> {
    if !component.first()?.is_ascii_digit() {
        return None;
    }
    Some(
        component
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .fold(0i64, |value, byte| {
                value
                    .saturating_mul(10)
                    .saturating_add(i64::from(byte - b'0'))
            }),
    )
}

fn qualifier(component: &[u8]) -> i32 {
    for (prefix, order) in [
        (b"dev".as_slice(), 0),
        (b"a", 1),
        (b"b", 2),
        (b"RC", 3),
        (b"rc", 3),
        (b"#", 4),
        (b"p", 5),
    ] {
        if component.starts_with(prefix) {
            return order;
        }
    }
    -1
}
