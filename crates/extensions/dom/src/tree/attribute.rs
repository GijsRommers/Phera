//! Keep an attribute's string value and its DOM text children in agreement.
//!
//! ```php
//! $attr = $root->getAttributeNode('id');
//! $attr->firstChild->data = 'new-id';
//! echo $attr->value;
//! ```

use super::{Arena, Kind, Name, Node, NodeId};

impl Arena {
    pub(crate) fn attribute(&mut self, node: Node, with_empty_child: bool) -> NodeId {
        let attribute = self.allocate(node);
        if with_empty_child || !self[attribute].value.is_empty() {
            self.add_attribute_text(attribute);
        }
        attribute
    }

    pub(crate) fn attribute_parts(&mut self, node: Node, parts: Vec<Node>) -> NodeId {
        if parts.is_empty() {
            return self.attribute(node, true);
        }
        let attribute = self.allocate(node);
        for child in parts {
            let child = self.allocate(child);
            self.link(attribute, child, None);
        }
        attribute
    }

    pub(crate) fn assign_attribute(&mut self, attribute: NodeId, value: String) {
        while let Some(child) = self[attribute].first_child() {
            self.detach(child);
        }
        self[attribute].value = value;
        self.add_attribute_text(attribute);
    }

    fn add_attribute_text(&mut self, attribute: NodeId) {
        let text = self[attribute].value.clone();
        let child = self.allocate(Node::new(Kind::Text, Name::new("#text"), text));
        self.link(attribute, child, None);
    }

    pub(crate) fn refresh_attribute(&mut self, attribute: NodeId) {
        if self[attribute].kind == Kind::Attribute {
            let mut value = String::new();
            for child in self.children(attribute) {
                value.push_str(&self[child].value);
            }
            self[attribute].value = value;
        }
    }

    pub(crate) fn refresh_parent_attribute(&mut self, id: NodeId) {
        if let Some(parent) = self[id].parent() {
            self.refresh_attribute(parent);
        }
    }
}
