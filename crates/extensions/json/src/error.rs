//! The JSON errors, `json_last_error()` and `json_last_error_msg()`.
//!
//! ```php
//! json_decode('[');
//! json_last_error();                 // JSON_ERROR_SYNTAX, 4
//! json_decode('[', flags: JSON_THROW_ON_ERROR);
//! // JsonException "Syntax error", code 4; json_last_error() stays 4
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, Value};

/// The values of PHP's `JSON_ERROR_*` constants. The error of the last call
/// is kept in the request state.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
#[repr(i64)]
pub(crate) enum Error {
    #[default]
    None = 0,
    Depth = 1,
    State = 2,
    Control = 3,
    Syntax = 4,
    Utf8 = 5,
    Recursion = 6,
    Infinite = 7,
    Unsupported = 8,
    Property = 9,
    Utf16 = 10,
    NonBackedEnum = 11,
}

impl Error {
    /// The text of `json_last_error_msg()` and of a `JsonException`.
    pub fn message(self) -> &'static str {
        match self {
            Self::None => "No error",
            Self::Depth => "Maximum stack depth exceeded",
            Self::State => "State mismatch (invalid or malformed JSON)",
            Self::Control => "Control character error, possibly incorrectly encoded",
            Self::Syntax => "Syntax error",
            Self::Utf8 => "Malformed UTF-8 characters, possibly incorrectly encoded",
            Self::Recursion => "Recursion detected",
            Self::Infinite => "Inf and NaN cannot be JSON encoded",
            Self::Unsupported => "Type is not supported",
            Self::Property => "The decoded property name is invalid",
            Self::Utf16 => "Single unpaired UTF-16 surrogate in unicode escape",
            Self::NonBackedEnum => "Non-backed enums have no default serialization",
        }
    }
}

/// A decoding failure can name a token's location. Encoding errors have no source token.
#[derive(Clone, Copy, Default)]
pub(crate) struct Details {
    code: Error,
    location: Option<(usize, usize)>,
}

impl From<Error> for Details {
    fn from(code: Error) -> Self {
        Self {
            code,
            location: None,
        }
    }
}

impl Details {
    pub(crate) fn at(code: Error, source: &[u8], offset: usize) -> Self {
        let prefix = source.get(..offset).unwrap_or(source);
        let line = 1 + prefix.iter().filter(|&&byte| byte == b'\n').count();
        let column = 1 + prefix
            .iter()
            .rev()
            .take_while(|&&byte| byte != b'\n')
            .filter(|&&byte| byte & 0xc0 != 0x80)
            .count();
        Self {
            code,
            location: Some((line, column)),
        }
    }

    fn message(self) -> std::borrow::Cow<'static, str> {
        let message = self.code.message();
        match self.location {
            Some((line, column)) => format!("{message} near location {line}:{column}").into(),
            None => message.into(),
        }
    }
}

/// Ends a call: records `error` as the last error, or, when `throw` is
/// set, throws it instead. A throwing call leaves the last error as it was,
/// even when it succeeds.
pub(crate) fn finish(
    context: &mut dyn Context,
    error: Error,
    throw: bool,
    value: Value,
) -> NativeResult {
    finish_details(context, error.into(), throw, value)
}

pub(crate) fn finish_details(
    context: &mut dyn Context,
    error: Details,
    throw: bool,
    value: Value,
) -> NativeResult {
    if throw {
        if error.code != Error::None {
            // The exception's code is the same number json_last_error()
            // would report.
            return Err(NativeError::throw_with_code(
                "JsonException",
                error.message().into_owned(),
                error.code as i64,
            ));
        }
    } else {
        *context.state().get_or_default::<Details>() = error;
    }
    Ok(value)
}

/// `json_last_error()`. Reading the error does not clear it.
pub(crate) fn last(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Int(
        context.state().get_or_default::<Details>().code as i64,
    ))
}

/// `json_last_error_msg()`.
pub(crate) fn message(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(
        context
            .state()
            .get_or_default::<Details>()
            .message()
            .into_owned(),
    ))
}
