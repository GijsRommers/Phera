//! Traverse the document without using the Rust stack for nesting.
//!
//! ```php
//! $all = $document->getElementsByTagName('*');
//! ```

use super::{Arena, Kind, NodeId};

impl Arena {
    pub fn children(&self, parent: NodeId) -> Children<'_> {
        Children {
            arena: self,
            next: self[parent].first,
        }
    }

    pub fn descendants(&self, root: NodeId) -> Descendants<'_> {
        Descendants {
            arena: self,
            root,
            next: self[root].first,
            skip_declarations: false,
        }
    }

    pub(crate) fn content_descendants(&self, root: NodeId) -> Descendants<'_> {
        Descendants {
            skip_declarations: true,
            ..self.descendants(root)
        }
    }

    pub fn ancestors(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(self[node].parent, |&id| self[id].parent)
    }
}

pub struct Children<'a> {
    arena: &'a Arena,
    next: Option<NodeId>,
}

impl Iterator for Children<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let id = self.next?;
        self.next = self.arena[id].next;
        Some(id)
    }
}

pub struct Descendants<'a> {
    arena: &'a Arena,
    root: NodeId,
    next: Option<NodeId>,
    skip_declarations: bool,
}

impl Iterator for Descendants<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        let id = self.next?;
        self.next = if self.skip_declarations
            && matches!(
                self.arena[id].kind,
                Kind::Doctype | Kind::Entity | Kind::EntityReference
            ) {
            None
        } else {
            self.arena[id].first
        };
        if self.next.is_none() {
            let mut cursor = id;
            loop {
                if let Some(next) = self.arena[cursor].next {
                    self.next = Some(next);
                    break;
                }
                match self.arena[cursor].parent {
                    Some(parent) if parent != self.root => cursor = parent,
                    _ => break,
                }
            }
        }
        Some(id)
    }
}
