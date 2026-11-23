//! `mail($to, $subject, $body, ['From' => $from])` composes RFC 2822 fields.
//! A supplied field value may fold across lines with leading whitespace.

use phera_extension_api::{ArrayKey, NativeError, Value};

pub fn additional_headers(function: &str, value: Option<&Value>) -> Result<Vec<u8>, NativeError> {
    let mut headers = Vec::new();
    match value {
        Some(Value::InternedStr(text)) => {
            let bytes = text.as_bytes();
            if bytes.contains(&0) {
                return Err(NativeError::value_error(format!(
                    "{function}(): Argument #4 ($additional_headers) must not contain any null bytes"
                )));
            }
            let end = bytes
                .iter()
                .rposition(|byte| !b" \r\n\t\x0b".contains(byte))
                .map_or(0, |index| index + 1);
            headers.extend_from_slice(&bytes[..end]);
        }
        Some(Value::String(text)) => {
            let bytes = text.as_bytes();
            if bytes.contains(&0) {
                return Err(NativeError::value_error(format!(
                    "{function}(): Argument #4 ($additional_headers) must not contain any null bytes"
                )));
            }
            let end = bytes
                .iter()
                .rposition(|byte| !b" \r\n\t\x0b".contains(byte))
                .map_or(0, |index| index + 1);
            headers.extend_from_slice(&bytes[..end]);
        }
        Some(Value::Array(fields)) => {
            for (key, value) in fields.iter() {
                let key = match key {
                    ArrayKey::Int(index) => {
                        return Err(NativeError::type_error(format!(
                            "Header name cannot be numeric, {index} given"
                        )));
                    }
                    ArrayKey::Interned(key) => key.to_php_string(),
                    ArrayKey::String(key) => key,
                };
                let name = key.as_bytes();
                let lower = name.to_ascii_lowercase();
                for disallowed in ["To", "Subject"] {
                    if name.eq_ignore_ascii_case(disallowed.as_bytes()) {
                        return Err(NativeError::value_error(format!(
                            "The additional headers cannot contain the \"{disallowed}\" header"
                        )));
                    }
                }
                match &*value.deref() {
                    Value::InternedStr(value) => field(&mut headers, name, value.as_bytes())?,
                    Value::String(value) => field(&mut headers, name, value.as_bytes())?,
                    Value::Array(values) => {
                        if matches!(
                            lower.as_slice(),
                            b"orig-date"
                                | b"from"
                                | b"sender"
                                | b"reply-to"
                                | b"cc"
                                | b"bcc"
                                | b"message-id"
                                | b"references"
                                | b"in-reply-to"
                        ) {
                            return Err(NativeError::type_error(format!(
                                "Header \"{}\" must be of type string, array given",
                                display(&lower)
                            )));
                        }
                        for (key, value) in values.iter() {
                            if let Some(key) = key.as_str_bytes() {
                                return Err(NativeError::type_error(format!(
                                    "Header \"{}\" must only contain numeric keys, \"{}\" found",
                                    display(name),
                                    display(key)
                                )));
                            }
                            let value = value.deref();
                            let Some(value_bytes) = value.as_str_bytes() else {
                                return Err(NativeError::type_error(format!(
                                    "Header \"{}\" must only contain values of type string, {} found",
                                    display(name),
                                    value_name(&value)
                                )));
                            };
                            field(&mut headers, name, value_bytes)?;
                        }
                    }
                    other => {
                        return Err(NativeError::type_error(format!(
                            "Header \"{}\" must be of type array|string, {} given",
                            display(name),
                            value_name(other)
                        )));
                    }
                }
            }
            headers.truncate(headers.len().saturating_sub(2));
        }
        _ => {}
    }
    Ok(headers)
}

fn field(headers: &mut Vec<u8>, name: &[u8], value: &[u8]) -> Result<(), NativeError> {
    if !name.iter().copied().all(field_name_byte) {
        return Err(NativeError::value_error(format!(
            "Header name \"{}\" contains invalid characters",
            display(name)
        )));
    }
    let mut index = 0;
    while index < value.len() {
        let folding = |position| matches!(value.get(position), Some(b' ' | b'\t'));
        let invalid = match value[index] {
            b'\r' if value.get(index + 1) != Some(&b'\n') => {
                Some("CR character that is not allowed in the header")
            }
            b'\r' if folding(index + 2) => {
                index += 2;
                None
            }
            b'\r' => Some(
                "CRLF characters that are used as a line separator and are not allowed in the header",
            ),
            b'\n' if folding(index + 1) => {
                index += 1;
                None
            }
            b'\n' => Some("LF character that is not allowed in the header"),
            0 => Some("NULL character that is not allowed in the header"),
            _ => None,
        };
        if let Some(invalid) = invalid {
            return Err(NativeError::value_error(format!(
                "Header \"{}\" contains {invalid}",
                display(name)
            )));
        }
        index += 1;
    }
    headers.extend_from_slice(name);
    headers.extend_from_slice(b": ");
    headers.extend_from_slice(value);
    headers.extend_from_slice(b"\r\n");
    Ok(())
}

fn field_name_byte(byte: u8) -> bool {
    matches!(byte, 33..=126) && byte != b':'
}

/// String headers can contain separate fields, with no empty or malformed lines.
pub(super) fn malformed_newlines(headers: &[u8]) -> bool {
    let Some(first) = headers.first().copied() else {
        return false;
    };
    if !field_name_byte(first) {
        return true;
    }
    let mut position = 0;
    while position < headers.len() {
        let next = headers.get(position + 1);
        match headers[position] {
            b'\r' => {
                if matches!(next, None | Some(b'\r'))
                    || next == Some(&b'\n')
                        && matches!(headers.get(position + 2), None | Some(b'\r' | b'\n'))
                {
                    return true;
                }
                position += 2;
            }
            b'\n' => {
                if matches!(next, None | Some(b'\r' | b'\n')) {
                    return true;
                }
                position += 2;
            }
            _ => position += 1,
        }
    }
    false
}

fn display(bytes: &[u8]) -> String {
    String::from_utf8_lossy(crate::command::c_prefix(bytes)).into_owned()
}

fn value_name(value: &Value) -> String {
    match &*value.deref() {
        Value::Bool(value) => value.to_string(),
        value => value.type_name(),
    }
}
