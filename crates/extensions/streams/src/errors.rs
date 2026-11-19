//! PHP separates rejected arguments from I/O failures.
//!
//! `fopen("/missing", "r")` warns and returns false. A NUL in the path throws.

use std::io;

use phera_extension_api::{Context, NativeError, NativeResult, Value};

pub fn check_path(function: &str, argument: &str, value: &[u8]) -> Result<(), NativeError> {
    if value.contains(&0) {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument {argument} must not contain any null bytes"
        )));
    }
    Ok(())
}

pub fn nonempty_path(value: &[u8]) -> Result<(), NativeError> {
    if value.is_empty() {
        return Err(NativeError::value_error("Path must not be empty"));
    }
    Ok(())
}

pub fn reason(error: &io::Error) -> String {
    let reason = error.to_string();
    match reason.rsplit_once(" (os error ") {
        Some((description, _)) => description.to_owned(),
        None => reason,
    }
}

pub fn warn_false(context: &mut dyn Context, message: String) -> NativeResult {
    context.warning(message);
    Ok(Value::Bool(false))
}

pub(crate) fn open_failure_message(function: &str, path: &[u8], reason: &[u8]) -> Vec<u8> {
    let prefix = format!("{function}(");
    [
        prefix.as_bytes(),
        path,
        b"): Failed to open stream: ",
        reason,
    ]
    .concat()
}

pub fn wrapper_warning(context: &mut dyn Context, function: &str, scheme: &[u8]) {
    context.warning(wrapper_message(function, scheme));
}

pub(crate) fn wrapper_message(function: &str, scheme: &[u8]) -> String {
    let scheme = String::from_utf8_lossy(scheme);
    format!(
        concat!(
            "{}(): Unable to find the wrapper \"{}\" - did you forget to enable ",
            "it when you configured PHP?"
        ),
        function, scheme
    )
}

pub fn nonnegative(function: &str, argument: &str, value: i64) -> Result<u64, NativeError> {
    u64::try_from(value).map_err(|_| {
        NativeError::value_error(format!(
            "{function}(): Argument {argument} must be greater than or equal to 0"
        ))
    })
}
