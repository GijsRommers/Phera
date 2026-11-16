//! Decode entry points and the single pending VM object-construction boundary.
mod parser;
mod scanner;
use crate::{
    error::{self, Error},
    flags::{Flags, INVALID_UTF8_IGNORE},
};
use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};
/// TODO: replace with Context::create_std_class(properties) once the VM offers it.
/// This deliberately does not invent a class identity owned by the extension.
fn object_from_properties(properties: PhpArray) -> Value {
    Value::from(properties)
}
/// Decode's explicit associative argument overrides JSON_OBJECT_AS_ARRAY.
pub(crate) fn json_decode(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let bytes = args::bytes(arguments, 0);
    let mut flags = Flags::new(args::int(arguments, 3).unwrap_or(0));
    if let Some(associative) = args::bool(arguments, 1) {
        flags.associative = associative;
    }
    if !flags.throw {
        *context.state().get_or_default::<Error>() = Error::None;
    }
    if bytes.is_empty() {
        return error::finish(context, Error::Syntax, flags.throw, Value::Null);
    }
    let depth = depth(args::int(arguments, 2).unwrap_or(512), "json_decode", 3)?;
    let result = parser::parse(bytes, depth, flags, object_from_properties);
    match result {
        Ok(value) => error::finish(context, Error::None, flags.throw, value),
        Err(error) => error::finish(context, error, flags.throw, Value::Null),
    }
}
/// Validate accepts only IGNORE, and does not reject NUL-prefixed object keys.
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
    let mut flags = Flags::new(bits);
    flags.associative = true;
    let result = parser::parse(bytes, depth, flags, object_from_properties);
    let error = result.as_ref().err().copied().unwrap_or_default();
    error::finish(context, error, false, Value::Bool(result.is_ok()))
}
/// PHP bounds parser depths to a positive signed C integer.
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
