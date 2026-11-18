//! Write deterministic builtin records: functions retain signatures and source locations.
//! For example, `strlen` points to its PHP 8.5 stub rather than a runtime implementation.

use super::fields::*;
use crate::index::*;

pub fn encode(index: &Index) -> String {
    let mut output = String::new();
    for (name, class) in &index.classes {
        output.push_str(&class_line(name, class));
        for (method, signature) in &class.methods {
            output.push_str(&signature_line(
                "M",
                &format!("{name}::{method}"),
                signature,
            ));
        }
        for (property, info) in &class.properties {
            output.push_str(&format!(
                "P\t{name}::{property}\t{}\t{}\t{}\n",
                encode_type(&info.ty),
                flags(info.modifiers),
                location_text(&info.location)
            ));
        }
        for (constant, info) in &class.constants {
            output.push_str(&constant_line("K", &format!("{name}::{constant}"), info));
        }
    }
    for (name, signature) in &index.functions {
        output.push_str(&signature_line("F", name, signature));
    }
    for (name, info) in &index.constants {
        output.push_str(&constant_line("G", name, info));
    }
    output
}

fn class_line(name: &str, class: &ClassInfo) -> String {
    format!(
        "C\t{name}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        class.name,
        class.kind as u8,
        flags(class.modifiers),
        class.parents.join(","),
        class.interfaces.join(","),
        class.traits.join(","),
        class.open_properties,
        location_text(&class.location)
    )
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
        "{kind}\t{name}\t{parameters}\t{}\t{}\t{}\t{}\t{}\n",
        encode_type(&signature.returns),
        flags(signature.modifiers),
        signature.abstract_body,
        signature.accepts_extra,
        location_text(&signature.location)
    )
}

fn constant_line(kind: &str, name: &str, info: &ConstantInfo) -> String {
    format!(
        "{kind}\t{name}\t{}\t{}\t{}\n",
        encode_type(&info.ty),
        flags(info.modifiers),
        location_text(&info.location)
    )
}
