//! Preserve the internal subset while keeping external identifiers inert.
//!
//! ```php
//! echo $document->doctype->internalSubset;
//! ```

use super::{Parser, declarations};
use crate::document::ParseError;
use crate::tree::{Doctype, DtdEntry, Kind, Node, NodeId};

impl Parser<'_> {
    pub(super) fn add_doctype(&mut self, text: &str, line: usize) -> Result<(), ParseError> {
        let text = text.trim();
        let end = text
            .find([' ', '\r', '\t', '\n', '['])
            .unwrap_or(text.len());
        let (name, rest) = text.split_at(end);
        let mut tail = rest.trim();
        let mut external = DtdEntry::default();
        if let Some(rest) = declarations::external(tail, &mut external) {
            tail = rest.trim();
        }

        let mut doctype = Doctype {
            public_id: external.public_id.unwrap_or_default(),
            system_id: external.system_id.unwrap_or_default(),
            ..Doctype::default()
        };
        let id = self.add(Kind::Doctype, name, "", line)?;
        if let Some(subset) = tail
            .strip_prefix('[')
            .and_then(|value| value.strip_suffix(']'))
        {
            doctype.internal_subset = Some(self.read_subset(id, subset, &mut doctype.notations));
        }
        self.document.arena[id].doctype = Some(Box::new(doctype));
        self.doctype = true;
        Ok(())
    }

    fn read_subset(&mut self, id: NodeId, mut subset: &str, notations: &mut String) -> String {
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
            self.read_declaration(id, declaration, notations, &mut serialized);
            subset = rest;
        }
        serialized
    }

    fn read_declaration(
        &mut self,
        id: NodeId,
        declaration: &str,
        notations: &mut String,
        serialized: &mut String,
    ) {
        let data = declaration.strip_suffix('>').unwrap_or(declaration);
        if let Some(data) = data.strip_prefix("<!ENTITY") {
            self.entities.declare(data);
            if let Some((markup, node)) = declarations::entity(data) {
                serialized.push_str(&markup);
                if let Some(node) = node {
                    self.add_entry(id, node);
                }
                return;
            }
        } else if let Some(data) = data.strip_prefix("<!ATTLIST") {
            self.attribute_rules.declare(data);
        } else if let Some(data) = data.strip_prefix("<!NOTATION")
            && let Some((markup, node)) = declarations::notation(data)
        {
            notations.push_str(&markup);
            self.add_entry(id, node);
            return;
        }
        serialized.push_str(declaration);
        serialized.push('\n');
    }

    fn add_entry(&mut self, owner: NodeId, node: Node) {
        let arena = &mut self.document.arena;
        if arena
            .children(owner)
            .any(|id| arena[id].kind == node.kind && arena[id].name == node.name)
        {
            return;
        }
        let id = arena.allocate(node);
        let _ = arena.insert(owner, id, None);
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
