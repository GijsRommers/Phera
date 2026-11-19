//! Represent entity references without creating cycles in the document tree.
//!
//! ```php
//! $declaration = $document->documentElement->firstChild->firstChild;
//! ```

use super::{Arena, Kind, Name, Node, NodeId};

impl Arena {
    pub(crate) fn entity(&self, name: &str) -> Option<NodeId> {
        let doctype = self
            .children(Self::DOCUMENT)
            .find(|&id| self[id].kind == Kind::Doctype)?;
        self.children(doctype)
            .find(|&id| self[id].kind == Kind::Entity && self[id].name.local == name)
    }

    pub(crate) fn reference_declaration(&self, node: NodeId) -> Option<NodeId> {
        self[node]
            .first_child()
            .or_else(|| self.entity(&self[node].name.local))
    }

    pub(crate) fn reference_text(&self, node: NodeId) -> &str {
        let entry = &self[node];
        let Some(declaration) = self.reference_declaration(node) else {
            return "";
        };
        if !entry.value.is_empty() {
            return &entry.value;
        }
        self[declaration]
            .first_child()
            .map_or("", |id| self[id].value.as_str())
    }

    pub(crate) fn attach_predefined_entity(&mut self, reference: NodeId) {
        if !matches!(
            self[reference].name.local.as_str(),
            "lt" | "gt" | "amp" | "apos" | "quot"
        ) {
            return;
        }
        let declaration = self.allocate(Node::new(Kind::Entity, self[reference].name.clone(), ""));
        self.link(reference, declaration, None);
    }

    pub(crate) fn materialize_entity(&mut self, name: &str, text: &str) {
        if let Some(declaration) = self.entity(name)
            && self[declaration].first_child().is_none()
            && !text.is_empty()
        {
            let child = self.allocate(Node::new(Kind::Text, Name::new("#text"), text));
            self.link(declaration, child, None);
        }
    }
}
