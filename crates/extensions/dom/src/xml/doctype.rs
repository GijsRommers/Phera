//! Preserve a DOCTYPE and collect internal general-entity declarations.
//!
//! ```php
//! echo $document->doctype->internalSubset;
//! ```

use super::{Parser, entities::quoted};
use crate::document::ParseError;
use crate::tree::{Doctype, Kind, NodeId};

impl Parser<'_> {
    pub(super) fn add_doctype(&mut self, text: &str, line: usize) -> Result<(), ParseError> {
        let (name, rest) = text
            .trim()
            .split_once([' ', '\r', '\t', '\n', '['])
            .unwrap_or((text, ""));
        let tail = text.trim().get(name.len()..).unwrap_or(rest).trim();
        let (external, subset) = match tail.split_once('[') {
            Some((external, subset)) => (external.trim(), subset.trim_end().strip_suffix(']')),
            None => (tail, None),
        };

        let mut doctype = Doctype::default();
        if let Some(data) = external.strip_prefix("PUBLIC").map(str::trim_start)
            && let Some((public, rest)) = quoted(data)
        {
            doctype.public_id = public.into();
            if let Some((system, _)) = quoted(rest.trim_start()) {
                doctype.system_id = system.into();
            }
        } else if let Some(data) = external.strip_prefix("SYSTEM").map(str::trim_start)
            && let Some((system, _)) = quoted(data)
        {
            doctype.system_id = system.into();
        }
        doctype.internal_subset = subset.map(|subset| self.read_subset(subset));
        let id: NodeId = self.add(Kind::Doctype, name, "", line)?;
        self.document.arena[id].doctype = Some(doctype);
        self.doctype = true;
        Ok(())
    }

    fn read_subset(&mut self, mut subset: &str) -> String {
        let mut serialized = String::new();
        while let Some(start) = subset.find('<') {
            subset = &subset[start..];
            let end = if subset.starts_with("<!--") {
                subset.find("-->").map(|end| end + 3)
            } else {
                declaration_end(subset)
            };
            let Some(end) = end else { break };
            let (declaration, rest) = subset.split_at(end);
            if let Some(data) = declaration.strip_prefix("<!ENTITY") {
                self.entities.declare(data.trim_end_matches('>'));
            }
            serialized.push_str(declaration);
            serialized.push('\n');
            subset = rest;
        }
        serialized
    }
}

fn declaration_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (index, byte) in text.bytes().enumerate() {
        match (quote, byte) {
            (Some(active), current) if active == current => quote = None,
            (None, b'\'' | b'"') => quote = Some(byte),
            (None, b'>') => return Some(index + 1),
            _ => {}
        }
    }
    None
}
