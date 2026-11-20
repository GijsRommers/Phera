//! Keep detached descendants alive without retaining their unreferenced parents.
//!
//! ```php
//! $child = $document->documentElement->firstChild;
//! $document->documentElement->remove();
//! assert($child->parentNode === null);
//! ```

use std::collections::{HashMap, HashSet};
use std::rc::{Rc, Weak};

use crate::tree::NodeId;

use super::DomState;

#[derive(Default)]
pub(super) struct Detached {
    roots: HashSet<NodeId>,
    pins: HashMap<NodeId, Weak<()>>,
}

impl Detached {
    pub(super) fn fork(&self) -> Self {
        Self {
            roots: self.roots.clone(),
            pins: HashMap::new(),
        }
    }
}

impl DomState {
    pub(super) fn remove(&mut self, root: NodeId) {
        if self.document.arena[root].parent().is_none() {
            return;
        }
        self.document.arena.detach(root);
        if self.document.arena[root].first_child().is_some()
            || !self.document.arena[root].attributes().is_empty()
        {
            self.detached.roots.insert(root);
        }
    }

    pub(super) fn pin(&mut self, node: NodeId) -> Rc<()> {
        let handle = self.detached.pins.entry(node).or_default();
        if let Some(pin) = handle.upgrade() {
            return pin;
        }
        let pin = Rc::new(());
        *handle = Rc::downgrade(&pin);
        pin
    }

    pub(super) fn release_detached(&mut self) {
        if self.detached.roots.is_empty() {
            return;
        }
        let arena = &mut self.document.arena;
        let wrappers = &mut self.wrappers;
        let pins = &mut self.detached.pins;
        let roots = &mut self.detached.roots;
        let retained = |id: NodeId| {
            wrappers
                .get(&id)
                .is_some_and(|item| item.strong_count() != 0)
                || pins.get(&id).is_some_and(|item| item.strong_count() != 0)
        };
        let mut pending = Vec::new();
        roots.retain(|&root| {
            if arena[root].parent().is_some() {
                return false;
            }
            if retained(root) {
                return true;
            }
            pending.push(root);
            false
        });
        while let Some(node) = pending.pop() {
            while let Some(child) = arena[node]
                .first_child()
                .or_else(|| arena[node].attributes().last().copied())
            {
                arena.detach(child);
                if retained(child) {
                    if !self.document.settings.modern {
                        super::namespace::reconcile_detached(arena, child);
                    }
                    if arena[child].first_child().is_some() || !arena[child].attributes().is_empty()
                    {
                        roots.insert(child);
                    }
                } else {
                    pending.push(child);
                }
            }
        }
    }
}
