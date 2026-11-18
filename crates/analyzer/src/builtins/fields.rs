//! Primitive table fields: `@DateTime|null` encodes a nullable class type.
//! This represents PHP declarations such as `function f(?DateTime $date) {}`.

use crate::{index::Location, types::ValueType};
use phera_ast::{Modifiers, Visibility};

pub fn encode_type(ty: &ValueType) -> String {
    match ty {
        ValueType::Class(name) => format!("@{name}"),
        ValueType::Union(items) => items.iter().map(encode_type).collect::<Vec<_>>().join("|"),
        _ => format!("{ty:?}").to_ascii_lowercase(),
    }
}

pub fn decode_type(text: &str) -> ValueType {
    if text.contains('|') {
        return ValueType::Union(text.split('|').map(decode_type).collect());
    }
    if let Some(name) = text.strip_prefix('@') {
        return ValueType::Class(name.into());
    }
    match text {
        "null" => ValueType::Null,
        "bool" => ValueType::Bool,
        "int" => ValueType::Int,
        "float" => ValueType::Float,
        "string" => ValueType::String,
        "array" => ValueType::Array,
        "object" => ValueType::Object,
        "void" => ValueType::Void,
        "never" => ValueType::Never,
        "unknowncallable" => ValueType::UnknownCallable,
        _ => ValueType::Mixed,
    }
}

pub fn flags(m: Modifiers) -> String {
    format!(
        "{},{},{},{},{},{}",
        m.visibility.unwrap_or(Visibility::Public) as u8,
        m.is_static,
        m.is_abstract,
        m.is_final,
        m.is_readonly,
        match m.set_visibility {
            None => "-",
            Some(Visibility::Public) => "0",
            Some(Visibility::Protected) => "1",
            Some(Visibility::Private) => "2",
        }
    )
}

pub fn modifiers(text: &str) -> Modifiers {
    let parts: Vec<_> = text.split(',').collect();
    Modifiers {
        visibility: Some(match parts.first().copied() {
            Some("1") => Visibility::Protected,
            Some("2") => Visibility::Private,
            _ => Visibility::Public,
        }),
        is_static: parts.get(1) == Some(&"true"),
        is_abstract: parts.get(2) == Some(&"true"),
        is_final: parts.get(3) == Some(&"true"),
        is_readonly: parts.get(4) == Some(&"true"),
        set_visibility: match parts.get(5).copied() {
            Some("0") => Some(Visibility::Public),
            Some("1") => Some(Visibility::Protected),
            Some("2") => Some(Visibility::Private),
            _ => None,
        },
    }
}

pub fn location(text: &str) -> Location {
    let (file, line) = text.rsplit_once(':').unwrap_or((text, "0"));
    Location {
        file: file.into(),
        line: line.parse().unwrap_or(0),
    }
}

pub fn location_text(location: &Location) -> String {
    format!("{}:{}", location.file, location.line)
}

pub fn list(text: &str) -> Vec<String> {
    text.split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}
