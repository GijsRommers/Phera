//! Builtin signatures generated from PHP stubs, e.g. `strlen(string $string): int`.
//! The table is committed; compiling and checking never require PHP or php-src.

use crate::{index::*, types::ValueType};
use phera_ast::{ClassKind, Modifiers, Visibility};
use std::collections::BTreeMap;

pub fn load() -> Index {
    decode(include_str!("../data/php85.tsv"))
}

pub fn encode(index: &Index) -> String {
    let mut output = String::new();
    for (name, class) in &index.classes {
        output.push_str(&format!(
            "C\t{name}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            class.name,
            class.kind as u8,
            flags(class.modifiers),
            class.parents.join(","),
            class.interfaces.join(","),
            class.traits.join(","),
            class.open_properties
        ));
        for (method, signature) in &class.methods {
            output.push_str(&signature_line(
                "M",
                &format!("{name}::{method}"),
                signature,
            ));
        }
        for (property, info) in &class.properties {
            output.push_str(&format!(
                "P\t{name}::{property}\t{}\t{}\n",
                encode_type(&info.ty),
                flags(info.modifiers)
            ));
        }
        for constant in class.constants.keys() {
            output.push_str(&format!("K\t{name}::{constant}\n"));
        }
    }
    for (name, signature) in &index.functions {
        output.push_str(&signature_line("F", name, signature));
    }
    for name in index.constants.keys() {
        output.push_str(&format!("G\t{name}\n"));
    }
    output
}

fn signature_line(kind: &str, name: &str, signature: &Signature) -> String {
    let parameters = signature
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{}:{}:{}:{}:{}",
                p.name,
                encode_type(&p.ty),
                p.optional,
                p.variadic,
                p.by_reference
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{kind}\t{name}\t{parameters}\t{}\t{}\t{}\t{}\n",
        encode_type(&signature.returns),
        flags(signature.modifiers),
        signature.abstract_body,
        signature.accepts_extra
    )
}

fn encode_type(ty: &ValueType) -> String {
    match ty {
        ValueType::Class(name) => format!("@{name}"),
        ValueType::Union(items) => items.iter().map(encode_type).collect::<Vec<_>>().join("|"),
        _ => format!("{ty:?}").to_lowercase(),
    }
}

fn decode_type(text: &str) -> ValueType {
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
        _ => ValueType::Mixed,
    }
}

fn flags(m: Modifiers) -> String {
    format!(
        "{},{},{},{},{}",
        m.visibility.unwrap_or(Visibility::Public) as u8,
        m.is_static,
        m.is_abstract,
        m.is_final,
        m.is_readonly
    )
}

fn modifiers(text: &str) -> Modifiers {
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
        ..Modifiers::default()
    }
}

fn decode(data: &str) -> Index {
    let mut index = Index::default();
    for line in data.lines().filter(|l| !l.starts_with('#')) {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() < 2 {
            continue;
        }
        let name = fields[1];
        let location = Location {
            file: "php85".into(),
            line: 0,
        };
        match fields[0] {
            "C" if fields.len() == 9 => {
                index.classes.insert(
                    name.into(),
                    ClassInfo {
                        name: fields[2].into(),
                        kind: match fields[3] {
                            "1" => ClassKind::Interface,
                            "2" => ClassKind::Trait,
                            "3" => ClassKind::Enum,
                            _ => ClassKind::Class,
                        },
                        modifiers: modifiers(fields[4]),
                        parents: list(fields[5]),
                        interfaces: list(fields[6]),
                        traits: list(fields[7]),
                        methods: BTreeMap::new(),
                        properties: BTreeMap::new(),
                        constants: BTreeMap::new(),
                        location,
                        open_properties: fields[8] == "true",
                    },
                );
            }
            "F" | "M" if fields.len() == 7 => {
                let parameters = fields[2]
                    .split(',')
                    .filter(|p| !p.is_empty())
                    .filter_map(|p| {
                        let p: Vec<_> = p.split(':').collect();
                        (p.len() == 5).then(|| ParameterInfo {
                            name: p[0].into(),
                            ty: decode_type(p[1]),
                            optional: p[2] == "true",
                            variadic: p[3] == "true",
                            by_reference: p[4] == "true",
                        })
                    })
                    .collect();
                let signature = Signature {
                    parameters,
                    returns: decode_type(fields[3]),
                    location,
                    modifiers: modifiers(fields[4]),
                    abstract_body: fields[5] == "true",
                    accepts_extra: fields[6] == "true",
                };
                if fields[0] == "F" {
                    index.functions.insert(name.into(), signature);
                } else if let Some((class, method)) = name.split_once("::")
                    && let Some(class) = index.classes.get_mut(class)
                {
                    class.methods.insert(method.into(), signature);
                }
            }
            "P" if fields.len() == 4 => {
                if let Some((class, property)) = name.split_once("::")
                    && let Some(class) = index.classes.get_mut(class)
                {
                    class.properties.insert(
                        property.into(),
                        PropertyInfo {
                            ty: decode_type(fields[2]),
                            modifiers: modifiers(fields[3]),
                            location,
                        },
                    );
                }
            }
            "K" => {
                if let Some((class, constant)) = name.split_once("::")
                    && let Some(class) = index.classes.get_mut(class)
                {
                    class.constants.insert(constant.into(), location);
                }
            }
            "G" => {
                index.constants.insert(name.into(), location);
            }
            _ => {}
        }
    }
    index
}

fn list(text: &str) -> Vec<String> {
    text.split(',')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect()
}
