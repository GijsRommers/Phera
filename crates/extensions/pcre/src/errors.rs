//! `preg_last_error()` and `preg_last_error_msg()`: the error of the last
//! `preg_*` call in this request.
//!
//! ```php
//! preg_match('/./u', "\xff");
//! preg_last_error();       // PREG_BAD_UTF8_ERROR, 4
//! preg_last_error_msg();   // "Malformed UTF-8 characters, possibly incorrectly encoded"
//! ```

use pcre2_sys::{
    PCRE2_ERROR_BADUTFOFFSET, PCRE2_ERROR_DEPTHLIMIT, PCRE2_ERROR_JIT_STACKLIMIT,
    PCRE2_ERROR_MATCHLIMIT, PCRE2_ERROR_UTF8_ERR1, PCRE2_ERROR_UTF8_ERR21,
};
use phera_extension_api::{Context, NativeResult, Value};

/// The values of PHP's `PREG_*_ERROR` constants.
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

/// The PHP error for a PCRE2 match error, as `pcre_handle_exec_error` in
/// php-src maps them.
impl From<pcre2::Error> for Error {
    fn from(error: pcre2::Error) -> Self {
        match error.code() {
            PCRE2_ERROR_MATCHLIMIT => Self::Backtrack,
            PCRE2_ERROR_DEPTHLIMIT => Self::Recursion,
            PCRE2_ERROR_UTF8_ERR21..=PCRE2_ERROR_UTF8_ERR1 => Self::Utf8,
            PCRE2_ERROR_BADUTFOFFSET => Self::Utf8Offset,
            PCRE2_ERROR_JIT_STACKLIMIT => Self::JitStack,
            _ => Self::Internal,
        }
    }
}

/// Records the error of the current call.
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
