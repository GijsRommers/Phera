//! `json_decode` and `json_validate`.
//!
//! ```php
//! json_decode('{"a":[1,2]}');          // object(stdClass) {a: [1, 2]}
//! json_decode('{"a":[1,2]}', true);    // ['a' => [1, 2]]
//! json_validate('{"a":');              // false, JSON_ERROR_SYNTAX
//! ```
//!
//! Objects are made through the context, because classes belong to the VM.
mod parser;
mod scanner;
use crate::{
    error::{self, Error},
    flags::{DecodeFlags, INVALID_UTF8_IGNORE},
};
use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};
/// `$associative`, when given, overrides `JSON_OBJECT_AS_ARRAY`.
pub(crate) fn json_decode(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let bytes = args::bytes(arguments, 0);
    let mut flags = DecodeFlags::from_bits(args::int(arguments, 3).unwrap_or(0));
    if let Some(associative) = args::bool(arguments, 1) {
        flags.associative = associative;
    }
    if !flags.throw_on_error {
        *context.state().get_or_default::<Error>() = Error::None;
    }
    if bytes.is_empty() {
        return error::finish(context, Error::Syntax, flags.throw_on_error, Value::Null);
    }
    let depth = depth(args::int(arguments, 2).unwrap_or(512), "json_decode", 3)?;
    // A JSON object becomes a stdClass, which only the VM can create. A
    // context without classes gets arrays instead. create_object takes the
    // properties even when it fails, so the first object asks with none.
    let mut has_classes = None;
    let mut make_object = |properties: PhpArray| {
        let has_classes = *has_classes
            .get_or_insert_with(|| context.create_object("stdClass", PhpArray::new()).is_ok());
        if has_classes {
            context
                .create_object("stdClass", properties)
                .unwrap_or(Value::Null)
        } else {
            Value::from(properties)
        }
    };
    let result = parser::parse(bytes, depth, flags, &mut make_object);
    match result {
        Ok(value) => error::finish(context, Error::None, flags.throw_on_error, value),
        Err(error) => error::finish(context, error, flags.throw_on_error, Value::Null),
    }
}
/// Like decoding, but builds nothing. Of the flags only
/// `JSON_INVALID_UTF8_IGNORE` is allowed, and an object key starting with
/// NUL is valid here.
pub(crate) fn json_validate(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let bits = args::int(arguments, 2).unwrap_or(0);
    if bits != 0 && bits != INVALID_UTF8_IGNORE {
        return Err(NativeError::value_error(
            "json_validate(): Argument #3 ($flags) must be a valid flag (allowed flags: JSON_INVALID_UTF8_IGNORE)",
        ));
    }
    let bytes = args::bytes(arguments, 0);
    if bytes.is_empty() {
        return error::finish(context, Error::Syntax, false, Value::Bool(false));
    }
    *context.state().get_or_default::<Error>() = Error::None;
    let depth = depth(args::int(arguments, 1).unwrap_or(512), "json_validate", 2)?;
    let mut flags = DecodeFlags::from_bits(bits);
    flags.associative = true;
    // Validation never builds objects: it parses in associative mode.
    let mut never_called = |properties: PhpArray| Value::from(properties);
    let result = parser::parse(bytes, depth, flags, &mut never_called);
    let error = result.as_ref().err().copied().unwrap_or_default();
    error::finish(context, error, false, Value::Bool(result.is_ok()))
}
/// `$depth` must be from 1 to `i32::MAX`, PHP's C int.
fn depth(depth: i64, function: &str, argument: usize) -> Result<usize, NativeError> {
    let reason = if depth <= 0 {
        "must be greater than 0"
    } else if depth > i64::from(i32::MAX) {
        "must be less than 2147483647"
    } else {
        return Ok(depth as usize);
    };
    Err(NativeError::value_error(format!(
        "{function}(): Argument #{argument} ($depth) {reason}"
    )))
}

#[cfg(test)]
mod tests;
