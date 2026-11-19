//! Update parent and sibling links together during DOM mutation.
//!
//! ```php
//! $root->insertBefore($new, $root->firstChild);
//! ```

use super::{Arena, Kind, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkError {
    Hierarchy,
    MissingReference,
}

impl Arena {
    pub fn detach(&mut self, child: NodeId) {
        let Some(parent) = self[child].parent.take() else {
            return;
        };
        self.mark_changed();
        if self[child].kind == Kind::Attribute {
            self[parent].attributes.retain(|&id| id != child);
            return;
        }

        let previous = self[child].previous.take();
        let next = self[child].next.take();
        if let Some(previous) = previous {
            self[previous].next = next;
        } else {
            self[parent].first = next;
        }
        if let Some(next) = next {
            self[next].previous = previous;
        } else {
            self[parent].last = previous;
        }
        self.refresh_attribute(parent);
    }

    pub fn insert(
        &mut self,
        parent: NodeId,
        child: NodeId,
        before: Option<NodeId>,
    ) -> Result<(), LinkError> {
        self.check_insert(parent, child, before)?;
        if before != Some(child) {
            self.detach(child);
            self.link(parent, child, before);
            self.refresh_attribute(parent);
        }
        Ok(())
    }

    pub(crate) fn check_insert(
        &self,
        parent: NodeId,
        child: NodeId,
        before: Option<NodeId>,
    ) -> Result<(), LinkError> {
        if before
            .is_some_and(|id| self[id].parent != Some(parent) || self[id].kind == Kind::Attribute)
        {
            return Err(LinkError::MissingReference);
        }
        let allows_child = match self[parent].kind {
            Kind::Document => !matches!(
                self[child].kind,
                Kind::Document | Kind::Attribute | Kind::Fragment
            ),
            Kind::Element | Kind::Fragment => matches!(
                self[child].kind,
                Kind::Element
                    | Kind::Text
                    | Kind::Cdata
                    | Kind::Comment
                    | Kind::ProcessingInstruction
                    | Kind::EntityReference
            ),
            Kind::Attribute => matches!(self[child].kind, Kind::Text | Kind::EntityReference),
            _ => false,
        };
        if !allows_child || parent == child {
            return Err(LinkError::Hierarchy);
        }
        // Fresh parser nodes have no descendants and need no ancestor walk.
        if self[child].first.is_some() && self.ancestors(parent).any(|id| id == child) {
            return Err(LinkError::Hierarchy);
        }
        Ok(())
    }

    pub(super) fn link(&mut self, parent: NodeId, child: NodeId, before: Option<NodeId>) {
        self.mark_changed();
        let previous = match before {
            Some(before) => self[before].previous,
            None => self[parent].last,
        };
        let entry = &mut self[child];
        entry.parent = Some(parent);
        entry.previous = previous;
        entry.next = before;

        if let Some(previous) = previous {
            self[previous].next = Some(child);
        } else {
            self[parent].first = Some(child);
        }
        if let Some(next) = before {
            self[next].previous = Some(child);
        } else {
            self[parent].last = Some(child);
        }
    }

    pub fn set_attribute(
        &mut self,
        element: NodeId,
        attribute: NodeId,
        replacing: Option<NodeId>,
    ) -> Result<(), LinkError> {
        if self[element].kind != Kind::Element || self[attribute].kind != Kind::Attribute {
            return Err(LinkError::Hierarchy);
        }
        if replacing.is_some_and(|previous| {
            self[previous].parent != Some(element) || self[previous].kind != Kind::Attribute
        }) {
            return Err(LinkError::MissingReference);
        }
        if replacing == Some(attribute) {
            return Ok(());
        }
        self.detach(attribute);
        if let Some(previous) = replacing {
            let Some(slot) = self[element]
                .attributes
                .iter()
                .position(|&id| id == previous)
            else {
                return Err(LinkError::MissingReference);
            };
            self.mark_changed();
            self[element].attributes[slot] = attribute;
            self[previous].parent = None;
            self[attribute].parent = Some(element);
        } else {
            self.link_attribute(element, attribute);
        }
        Ok(())
    }

    pub(super) fn link_attribute(&mut self, element: NodeId, attribute: NodeId) {
        self.mark_changed();
        self[element].attributes.push(attribute);
        self[attribute].parent = Some(element);
    }
}
