//! JSON error messages and request-local last-error state.
use phera_extension_api::{Context, NativeError, NativeResult, Value};
/// PHP's stable error codes, also stored in request state.
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
}
impl Error {
    /// Messages shared by json_last_error_msg and JsonException.
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
        }
    }
}
/// Throwing calls preserve the previous last-error state, even on success.
pub(crate) fn finish(
    context: &mut dyn Context,
    error: Error,
    throw: bool,
    value: Value,
) -> NativeResult {
    if throw {
        if error != Error::None {
            return Err(NativeError::throw("JsonException", error.message()));
        }
    } else {
        *context.state().get_or_default::<Error>() = error;
    }
    Ok(value)
}
/// Report the request error without resetting it.
pub(crate) fn last(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Int(*context.state().get_or_default::<Error>() as i64))
}
/// Reading the descriptive message also preserves request state.
pub(crate) fn message(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(
        context.state().get_or_default::<Error>().message(),
    ))
}
