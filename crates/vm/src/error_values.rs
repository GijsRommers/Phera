//! Render scalar values as PHP does in exception messages and argument traces.

use phera_value::{Value, float_to_string_precision14};

/// PHP limits exception strings to fifteen source bytes before escaping them.
pub(crate) fn describe(value: &Value) -> String {
    describe_with_limit(value, 15)
}

/// Trace rendering honors the request setting while engine messages retain their own limit.
pub(crate) fn describe_with_limit(value: &Value, limit: usize) -> String {
    match &*value.deref() {
        Value::Null => "NULL".into(),
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Float(value) => {
            let mut text = float_to_string_precision14(*value);
            if value.is_finite() && !text.contains(['.', 'E']) {
                text.push_str(".0");
            }
            text
        }
        Value::String(value) => quoted_string(value.as_bytes(), limit),
        value => format!("of type {}", value.type_name()),
    }
}

/// Escape bytes before adding PHP's truncation marker, preserving embedded controls.
fn quoted_string(bytes: &[u8], limit: usize) -> String {
    let mut text = String::from("'");
    for byte in bytes.iter().take(limit) {
        match byte {
            b'\n' => text.push_str("\\n"),
            b'\r' => text.push_str("\\r"),
            b'\t' => text.push_str("\\t"),
            12 => text.push_str("\\f"),
            11 => text.push_str("\\v"),
            27 => text.push_str("\\e"),
            b'\\' => text.push_str("\\\\"),
            32..=126 => text.push(char::from(*byte)),
            byte => text.push_str(&format!("\\x{byte:02X}")),
        }
    }
    if bytes.len() > limit {
        text.push_str("...");
    }
    text.push('\'');
    text
}
