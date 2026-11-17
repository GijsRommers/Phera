//! `preg_replace_callback('/x/', 42, '')` rejects malformed callbacks before matching.

use phera_extension_api::{ArrayKey, NativeError, Value};

/// Full function/method resolution belongs to the VM; Context currently only exposes call.
pub(crate) fn validate(callback: &Value, array_function: bool) -> Result<(), NativeError> {
    let reason = match &*callback.deref() {
        Value::String(_) | Value::Object(_) => return Ok(()),
        Value::Array(array) => {
            if array.len() != 2 {
                "array callback must have exactly two members"
            } else if !array.contains_key(&ArrayKey::Int(0))
                || !array.contains_key(&ArrayKey::Int(1))
            {
                "array callback has to contain indices 0 and 1"
            } else if !array
                .get(&ArrayKey::Int(0))
                .is_some_and(|value| matches!(&*value.deref(), Value::String(_) | Value::Object(_)))
            {
                "first array member is not a valid class name or object"
            } else if !array
                .get(&ArrayKey::Int(1))
                .is_some_and(|value| value.deref().is_string())
            {
                "second array member is not a valid method"
            } else {
                return Ok(());
            }
        }
        _ => "no array or string given",
    };
    let message = if array_function {
        "preg_replace_callback_array(): Argument #1 ($pattern) \
         must contain only valid callbacks"
            .to_string()
    } else {
        format!(
            "preg_replace_callback(): Argument #2 ($callback) \
                 must be a valid callback, {reason}"
        )
    };
    Err(NativeError::type_error(message))
}
