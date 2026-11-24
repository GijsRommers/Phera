//! SQL parameters such as the values passed to mysqli_stmt::execute().

use std::fmt::Write as _;

#[derive(Clone, Debug)]
pub enum BoundValue {
    Null,
    Int(i64),
    String(Vec<u8>),
    Blob(Vec<u8>),
    Bool(bool),
}

pub fn literal(value: &BoundValue, mysql: bool) -> String {
    let mut output = String::new();
    push_literal(&mut output, value, mysql);
    output
}

pub fn push_literal(output: &mut String, value: &BoundValue, mysql: bool) {
    match value {
        BoundValue::Null => output.push_str("NULL"),
        BoundValue::Int(value) => {
            let _ = write!(output, "{value}");
        }
        BoundValue::Bool(value) => output.push_str(match (mysql, value) {
            (true, true) => "1",
            (true, false) => "0",
            (false, true) => "true",
            (false, false) => "false",
        }),
        BoundValue::String(bytes) | BoundValue::Blob(bytes) => push_quote(output, bytes, mysql),
    }
}

pub fn quote(bytes: &[u8], mysql: bool) -> String {
    let mut quoted = String::with_capacity(bytes.len() + 2);
    push_quote(&mut quoted, bytes, mysql);
    quoted
}

pub fn push_quote(quoted: &mut String, bytes: &[u8], mysql: bool) {
    quoted.push('\'');
    // Fast path for valid UTF-8 strings containing no bytes that require escaping.
    if let Ok(text) = std::str::from_utf8(bytes) {
        let needs_escape = if mysql {
            bytes
                .iter()
                .any(|&b| matches!(b, b'\'' | b'\\' | b'\0' | b'\n' | b'\r' | 0x1a))
        } else {
            bytes.contains(&b'\'')
        };
        if !needs_escape {
            quoted.push_str(text);
            quoted.push('\'');
            return;
        }
    }
    for character in String::from_utf8_lossy(bytes).chars() {
        match character {
            '\'' => quoted.push_str(if mysql { "\\'" } else { "''" }),
            '\\' if mysql => quoted.push_str("\\\\"),
            '\0' if mysql => quoted.push_str("\\0"),
            '\n' if mysql => quoted.push_str("\\n"),
            '\r' if mysql => quoted.push_str("\\r"),
            '\u{1a}' if mysql => quoted.push_str("\\Z"),
            _ => quoted.push(character),
        }
    }
    quoted.push('\'');
}
