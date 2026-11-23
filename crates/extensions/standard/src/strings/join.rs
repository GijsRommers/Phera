//! Joining arrays. `implode("-", ["a", "b"])` is `"a-b"`.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args, value::to_php_string,
};

use super::{buffer, reserve};
use crate::arguments::stringify;

pub(super) fn register_sync_selectors(registry: &mut Registry) {
    registry.register_sync_selector("implode", |slots| {
        can_join_sync(slots)
            .then_some(|context, arguments| join_sync(context, arguments, "implode"))
    });
    registry.register_sync_selector("join", |slots| {
        can_join_sync(slots).then_some(|context, arguments| join_sync(context, arguments, "join"))
    });
}

fn can_join_sync(slots: &[Option<Value>]) -> bool {
    let array = match slots {
        [Some(Value::Array(array))]
        | [Some(Value::Array(array)), Some(Value::Null)]
        | [
            Some(Value::InternedStr(_) | Value::String(_)),
            Some(Value::Array(array)),
        ] => array,
        _ => return false,
    };
    array
        .values()
        .all(|value| !matches!(value, Value::Object(_) | Value::Reference(_)))
}

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
pub(super) const IMPLODE: FunctionEntry = FunctionEntry::asynchronous(
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
pub(super) const JOIN: FunctionEntry = FunctionEntry::asynchronous(
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

fn operands<'a>(
    arguments: &'a [Value],
    function: &str,
) -> Result<(&'a [u8], &'a PhpArray), NativeError> {
    let operands: (&[u8], &PhpArray) = match (args::array(arguments, 0), args::array(arguments, 1))
    {
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
    Ok(operands)
}

fn join_sync(context: &mut dyn Context, arguments: &[Value], function: &str) -> NativeResult {
    let (separator, array) = operands(arguments, function)?;
    if array.len() == 1
        && let Some(value) = array.values().next()
    {
        return Ok(Value::String(to_php_string(value, context.diagnostics())?));
    }
    // Strings, integers, booleans and null have a length that is known
    // before any is written: the result is made at its size, once.
    if let Some(length) = joined_length(separator, array) {
        let mut result = buffer(length)?;
        for (index, value) in array.values().enumerate() {
            if index > 0 {
                result.extend_from_slice(separator);
            }
            match value {
                Value::InternedStr(string) => result.extend_from_slice(string.as_bytes()),
                Value::String(string) => result.extend_from_slice(string.as_bytes()),
                Value::Int(number) => {
                    phera_extension_api::value::push_integer(&mut result, *number)
                }
                Value::Bool(true) => result.push(b'1'),
                _ => {}
            }
        }
        return Ok(Value::from(result));
    }
    let mut result = buffer(0)?;
    for (index, value) in array.values().enumerate() {
        append_scalar(context, &mut result, separator_at(separator, index), value)?;
    }
    Ok(Value::from(result))
}

async fn join_values(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let (separator, array) = operands(&arguments, function)?;
    let mut result = Vec::new();
    for (index, value) in array.values().enumerate() {
        let string = stringify(context, value).await?;
        append_bytes(
            &mut result,
            separator_at(separator, index),
            string.as_bytes(),
        )?;
    }
    Ok(Value::from(result))
}

fn separator_at(separator: &[u8], index: usize) -> &[u8] {
    if index == 0 { &[] } else { separator }
}

/// The bytes the result takes when every value has a known length. Other
/// conversions build the result in order, so their warnings and errors keep
/// their place.
fn joined_length(separator: &[u8], array: &PhpArray) -> Option<usize> {
    let separators = separator
        .len()
        .saturating_mul(array.len().saturating_sub(1));
    array.values().try_fold(separators, |length, value| {
        let bytes = match value {
            Value::InternedStr(string) => string.len(),
            Value::String(string) => string.len(),
            Value::Int(number) => integer_length(*number),
            Value::Bool(boolean) => usize::from(*boolean),
            Value::Null => 0,
            _ => return None,
        };
        Some(length.saturating_add(bytes))
    })
}

fn integer_length(number: i64) -> usize {
    number.unsigned_abs().checked_ilog10().unwrap_or(0) as usize + 1 + usize::from(number < 0)
}

fn append_scalar(
    context: &mut dyn Context,
    result: &mut Vec<u8>,
    separator: &[u8],
    value: &Value,
) -> Result<(), NativeError> {
    match value {
        Value::InternedStr(string) => append_bytes(result, separator, string.as_bytes()),
        Value::String(string) => append_bytes(result, separator, string.as_bytes()),
        Value::Int(number) => {
            reserve(
                result,
                separator.len().saturating_add(integer_length(*number)),
            )?;
            result.extend_from_slice(separator);
            phera_extension_api::value::push_integer(result, *number);
            Ok(())
        }
        Value::Bool(true) => append_bytes(result, separator, b"1"),
        Value::Bool(false) | Value::Null => append_bytes(result, separator, b""),
        _ => {
            let string = to_php_string(value, context.diagnostics())?;
            append_bytes(result, separator, string.as_bytes())
        }
    }
}

fn append_bytes(result: &mut Vec<u8>, separator: &[u8], bytes: &[u8]) -> Result<(), NativeError> {
    reserve(result, separator.len().saturating_add(bytes.len()))?;
    result.extend_from_slice(separator);
    result.extend_from_slice(bytes);
    Ok(())
}
