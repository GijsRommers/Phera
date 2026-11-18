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

pub fn open_failed(context: &mut dyn Context, function: &str, path: &[u8], reason: &str) -> Value {
    let path = String::from_utf8_lossy(path);
    context.warning(format!(
        "{function}({path}): Failed to open stream: {reason}"
    ));
    Value::Bool(false)
}

pub fn wrapper_warning(context: &mut dyn Context, function: &str, scheme: &[u8]) {
    let scheme = String::from_utf8_lossy(scheme);
    context.warning(format!(
        concat!(
            "{}(): Unable to find the wrapper \"{}\" - did you forget to enable ",
            "it when you configured PHP?"
        ),
        function, scheme
    ));
}

pub fn nonnegative(function: &str, argument: &str, value: i64) -> Result<u64, NativeError> {
    u64::try_from(value).map_err(|_| {
        NativeError::value_error(format!(
            "{function}(): Argument {argument} must be greater than or equal to 0"
        ))
    })
}
