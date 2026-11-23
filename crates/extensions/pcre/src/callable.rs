//! The shape check PHP makes of a callback before it matches anything.
//!
//! ```php
//! preg_replace_callback('/x/', 42, '');
//! // TypeError: preg_replace_callback(): Argument #2 ($callback) must be a
//! // valid callback, no array or string given
//! ```
//!
//! Only the shape is checked here: whether the function or method exists is
//! for the VM to find out when it calls it.

use phera_extension_api::{ArrayKey, PhpArray, Value};

/// Why `callback` cannot be a callback, or `None` when its shape is right.
pub(crate) fn invalid_reason(callback: &Value) -> Option<&'static str> {
    match &*callback.deref() {
        Value::InternedStr(_) | Value::String(_) | Value::Object(_) => None,
        Value::Array(array) => invalid_array_reason(array),
        _ => Some("no array or string given"),
    }
}

/// `[$object, 'method']` or `['Class', 'method']`.
fn invalid_array_reason(array: &PhpArray) -> Option<&'static str> {
    let class = array.get(&ArrayKey::Int(0));
    let method = array.get(&ArrayKey::Int(1));
    let is_class = |value: &Value| {
        matches!(
            &*value.deref(),
            Value::InternedStr(_) | Value::String(_) | Value::Object(_)
        )
    };
    if array.len() != 2 {
        Some("array callback must have exactly two members")
    } else if class.is_none() || method.is_none() {
        Some("array callback has to contain indices 0 and 1")
    } else if !class.is_some_and(is_class) {
        Some("first array member is not a valid class name or object")
    } else if !method.is_some_and(|value| value.deref().is_string()) {
        Some("second array member is not a valid method")
    } else {
        None
    }
}
