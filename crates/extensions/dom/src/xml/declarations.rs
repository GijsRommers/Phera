//! Read entity and notation declarations from an internal DTD.
//!
//! ```php
//! echo $document->doctype->entities->getNamedItem('logo')->systemId;
//! ```

use std::fmt::Write;

use crate::tree::{DtdEntry, Kind, Name, Node};

use super::entities::quoted;

pub(super) fn entity(data: &str) -> Option<(String, Option<Node>)> {
    let data = data.trim();
    let (parameter, data) = match data.strip_prefix('%') {
        Some(data) => (true, data.trim_start()),
        None => (false, data),
    };
    let (name, rest) = data.split_once(char::is_whitespace)?;
    let rest = rest.trim_start();
    let mut serialized = format!("<!ENTITY {}{name}", if parameter { "% " } else { "" });
    let mut entry = DtdEntry::default();
    let value = if let Some((value, _)) = quoted(rest) {
        push_quoted(&mut serialized, value);
        value.to_owned()
    } else {
        let tail = external(rest, &mut entry)?;
        push_external(&mut serialized, &entry);
        if let Some(name) = tail.trim_start().strip_prefix("NDATA") {
            let name = name.trim();
            entry.notation = Some(name.into());
            let _ = write!(serialized, " NDATA {name}");
        }
        String::new()
    };
    serialized.push_str(">\n");
    let declaration = (!parameter).then(|| {
        let mut node = Node::new(Kind::Entity, Name::new(name), value);
        node.dtd_entry = Some(Box::new(entry));
        node
    });
    Some((serialized, declaration))
}

pub(super) fn notation(data: &str) -> Option<(String, Node)> {
    let (name, data) = data.trim().split_once(char::is_whitespace)?;
    let mut entry = DtdEntry::default();
    external(data.trim(), &mut entry)?;
    let mut serialized = format!("<!NOTATION {name}");
    push_external(&mut serialized, &entry);
    serialized.push_str(" >\n");
    let mut node = Node::new(Kind::Notation, Name::new(name), "");
    node.dtd_entry = Some(Box::new(entry));
    Some((serialized, node))
}

pub(super) fn external<'a>(data: &'a str, entry: &mut DtdEntry) -> Option<&'a str> {
    if let Some(data) = data.strip_prefix("PUBLIC") {
        let (public, rest) = quoted(data.trim_start())?;
        entry.public_id = Some(public.into());
        if let Some((system, rest)) = quoted(rest.trim_start()) {
            entry.system_id = Some(system.into());
            return Some(rest);
        }
        return Some(rest);
    }
    let (system, rest) = quoted(data.strip_prefix("SYSTEM")?.trim_start())?;
    entry.system_id = Some(system.into());
    Some(rest)
}

fn push_external(output: &mut String, entry: &DtdEntry) {
    if let Some(public) = &entry.public_id {
        output.push_str(" PUBLIC");
        push_quoted(output, public);
    } else {
        output.push_str(" SYSTEM");
    }
    if let Some(system) = &entry.system_id {
        push_quoted(output, system);
    }
}

fn push_quoted(output: &mut String, value: &str) {
    let quote = if value.contains('"') && !value.contains('\'') {
        '\''
    } else {
        '"'
    };
    let _ = write!(output, " {quote}{value}{quote}");
}
