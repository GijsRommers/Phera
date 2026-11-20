//! SQL parameters such as the values passed to mysqli_stmt::execute().

#[derive(Clone, Debug)]
pub enum BoundValue {
    Null,
    Int(i64),
    String(Vec<u8>),
    Blob(Vec<u8>),
    Bool(bool),
}

pub fn literal(value: &BoundValue, mysql: bool) -> String {
    match value {
        BoundValue::Null => "NULL".into(),
        BoundValue::Int(value) => value.to_string(),
        BoundValue::Bool(value) => match (mysql, value) {
            (true, true) => "1",
            (true, false) => "0",
            (false, true) => "true",
            (false, false) => "false",
        }
        .into(),
        BoundValue::String(bytes) | BoundValue::Blob(bytes) => quote(bytes, mysql),
    }
}

pub fn quote(bytes: &[u8], mysql: bool) -> String {
    let mut quoted = String::from("'");
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
    quoted
}
