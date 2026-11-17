//! Request-local errors: `preg_match('//u', "\\xff"); preg_last_error()` returns 4.

use phera_extension_api::{Context, NativeResult, Value};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(i64)]
pub(crate) enum Error {
    #[default]
    None = 0,
    Internal = 1,
    Backtrack = 2,
    Recursion = 3,
    Utf8 = 4,
    Utf8Offset = 5,
    JitStack = 6,
}
impl Error {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::None => "No error",
            Self::Internal => "Internal error",
            Self::Backtrack => "Backtrack limit exhausted",
            Self::Recursion => "Recursion limit exhausted",
            Self::Utf8 => "Malformed UTF-8 characters, possibly incorrectly encoded",
            Self::Utf8Offset => {
                "The offset did not correspond to the beginning of a valid UTF-8 code point"
            }
            Self::JitStack => "JIT stack limit exhausted",
        }
    }
}
impl From<pcre2::Error> for Error {
    fn from(error: pcre2::Error) -> Self {
        // PCRE2's public, stable execution error numbers; no sys/FFI calls.
        match error.code() {
            -47 => Self::Backtrack,
            -53 => Self::Recursion,
            -23..=-3 => Self::Utf8,
            -36 => Self::Utf8Offset,
            -46 => Self::JitStack,
            _ => Self::Internal,
        }
    }
}
pub(crate) fn set(context: &mut dyn Context, error: Error) {
    *context.state().get_or_default::<Error>() = error;
}
pub(crate) fn last(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Int(*context.state().get_or_default::<Error>() as i64))
}
pub(crate) fn message(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::from(
        context.state().get_or_default::<Error>().message(),
    ))
}
