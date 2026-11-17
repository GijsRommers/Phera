//! PHP 8.5's validation exceptions.
//!
//! ```php
//! try {
//!     filter_var('abc', FILTER_VALIDATE_INT, FILTER_THROW_ON_FAILURE);
//! } catch (Filter\FilterException $e) {
//!     echo $e->getMessage();
//! }
//! ```

use phera_extension_api::{ClassEntry, NativeError, Registry, Value};

const FILTER_EXCEPTION: &str = "Filter\\FilterException";
const FAILED_EXCEPTION: &str = "Filter\\FilterFailedException";

pub fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("Exception"),
        ..ClassEntry::class(FILTER_EXCEPTION)
    });
    registry.class(ClassEntry {
        parent: Some(FILTER_EXCEPTION),
        ..ClassEntry::class(FAILED_EXCEPTION)
    });
}

pub fn failed(reason: &str) -> NativeError {
    NativeError::throw(
        FAILED_EXCEPTION,
        format!("filter validation failed: {reason}"),
    )
}

pub fn got_scalar(value: &Value) -> NativeError {
    let name = match value {
        Value::Bool(true) => "true".into(),
        Value::Bool(false) => "false".into(),
        other => other.type_name(),
    };
    failed(&format!("not an array (got {name})"))
}

pub fn rejected(filter: &str, input: &[u8]) -> NativeError {
    let text = input.split(|&byte| byte == 0).next().unwrap_or_default();
    if let Ok(text) = std::str::from_utf8(text) {
        return failed(&format!("filter {filter} not satisfied by '{text}'"));
    }

    // The general native error message is UTF-8. The declared PHP property
    // can hold a byte string, so getMessage() still exposes the original bytes.
    let mut message =
        format!("filter validation failed: filter {filter} not satisfied by '").into_bytes();
    message.extend_from_slice(text);
    message.push(b'\'');
    NativeError::throw(FAILED_EXCEPTION, String::from_utf8_lossy(&message))
        .with_property("message", Value::from(message))
}
