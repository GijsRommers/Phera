//! Preserve the real document and node identity of PHP callback results.
//!
//! ```php
//! $xpath->registerPhpFunctions(['result' => fn () => $other->documentElement]);
//! ```

use std::collections::HashMap;
use std::rc::Rc;

use crate::tree::{Arena, NodeId};

use super::super::Location;

pub(super) struct Nodes {
    original_count: usize,
    sources: HashMap<NodeId, Location>,
    copies: HashMap<(usize, NodeId), NodeId>,
}

impl Nodes {
    pub(super) fn new(arena: &Arena) -> Self {
        Self {
            original_count: arena.node_count(),
            sources: HashMap::new(),
            copies: HashMap::new(),
        }
    }

    pub(super) fn location(&self, document: &Location, id: NodeId) -> Location {
        self.sources
            .get(&id)
            .cloned()
            .unwrap_or_else(|| document.at(id))
    }

    pub(super) fn copies_used(&self, ids: &[NodeId]) -> bool {
        ids.iter().any(|id| self.sources.contains_key(id))
    }

    pub(super) fn add(&mut self, arena: &mut Arena, document: &Location, node: Location) -> NodeId {
        let same_document = node.same_document(document);
        if same_document && node.id.index() < self.original_count {
            return node.id;
        }
        let source_key = Rc::as_ptr(&node.shared) as usize;
        if let Some(&copy) = self.copies.get(&(source_key, node.id)) {
            return copy;
        }
        let state = node.shared.borrow();
        let source = &state.document.arena;
        let root = if same_document {
            node.id
        } else {
            source.ancestors(node.id).last().unwrap_or(node.id)
        };
        let (fallback, mapping) = arena.import_mapping(source, root, true);
        let mut result = fallback;
        for (original, copy) in mapping {
            self.sources.insert(copy, node.at(original));
            self.copies.insert((source_key, original), copy);
            if original == node.id {
                result = copy;
            }
        }
        result
    }
}
