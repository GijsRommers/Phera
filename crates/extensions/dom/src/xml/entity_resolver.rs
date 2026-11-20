//! Share bounded entity expansion with XML readers that consume tokens directly.
//!
//! ```php
//! xml_parse($parser, '<!DOCTYPE r [<!ENTITY x "value">]><r>&x;</r>', true);
//! ```

use super::entities::Entities;
use crate::document::ParseError;
use crate::tree::{Arena, Kind, NodeId};

#[derive(Clone, Default)]
pub struct EntityResolver(Entities);

impl EntityResolver {
    pub fn from_doctype(arena: &Arena, doctype: NodeId) -> Self {
        let mut entities = Entities::default();
        for id in arena.children(doctype) {
            let node = &arena[id];
            if node.kind == Kind::Entity {
                let external = node
                    .dtd_entry
                    .as_ref()
                    .is_some_and(|entry| entry.system_id.is_some());
                let value = (!external).then(|| node.value.clone());
                entities.define(node.name.local.clone(), value);
            }
        }
        Self(entities)
    }

    pub fn text(&mut self, name: &str) -> Result<String, ParseError> {
        self.0.resolve(name).map(|(text, _)| text)
    }

    pub fn attribute(&mut self, input: &str) -> Result<String, ParseError> {
        self.0.attribute(input, true).map(|value| value.text)
    }
}
