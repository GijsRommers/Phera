//! Canonical XML orders namespace declarations and attributes.
//!
//! ```php
//! $document->loadXML('<r z="1" a="2"/>');
//! echo $document->C14N(); // <r a="2" z="1"></r>
//! ```

use std::collections::{BTreeMap, BTreeSet};

use super::{Escaping, Serializer, escape};
use crate::tree::{Kind, Node, NodeId};
use crate::xml::names::XMLNS;

impl Serializer<'_> {
    pub(super) fn canonical_attributes(
        &self,
        root: NodeId,
        id: NodeId,
        exclusive: bool,
        output: &mut String,
    ) {
        let node = &self.arena[id];
        let mut namespaces = BTreeMap::new();
        let mut attributes = Vec::new();
        for &attribute in node.attributes() {
            let attribute = &self.arena[attribute];
            if attribute.name.namespace.as_deref() == Some(XMLNS) {
                namespaces.insert(prefix(attribute), attribute.value.as_str());
            } else {
                attributes.push(attribute);
            }
        }
        if root == id || exclusive {
            for ancestor in self.arena.ancestors(id) {
                for &attribute in self.arena[ancestor].attributes() {
                    let attribute = &self.arena[attribute];
                    if attribute.name.namespace.as_deref() == Some(XMLNS) {
                        namespaces
                            .entry(prefix(attribute))
                            .or_insert(attribute.value.as_str());
                    }
                }
            }
        }
        if exclusive {
            let used: BTreeSet<_> = attributes
                .iter()
                .filter_map(|attribute| attribute.name.prefix.as_deref())
                .chain([node.name.prefix.as_deref().unwrap_or("")])
                .collect();
            namespaces.retain(|prefix, _| used.contains(prefix));
        }
        for (prefix, value) in namespaces {
            if prefix == "xml" {
                continue;
            }
            output.push_str(" xmlns");
            if !prefix.is_empty() {
                output.push(':');
                output.push_str(prefix);
            }
            output.push_str("=\"");
            escape(output, value, Escaping::Canonical { attribute: true });
            output.push('"');
        }
        attributes.sort_by(|left, right| {
            (&left.name.namespace, &left.name.local)
                .cmp(&(&right.name.namespace, &right.name.local))
        });
        for attribute in attributes {
            self.attribute(attribute, output);
        }
    }

    pub(super) fn canonical_before(&self, id: NodeId, output: &mut String) {
        if !matches!(self.mode, super::Mode::Canonical { .. }) {
            return;
        }
        let node = &self.arena[id];
        if node
            .parent()
            .is_some_and(|parent| self.arena[parent].kind == Kind::Document)
            && previous_elements(self.arena, id)
        {
            output.push('\n');
        }
    }
}

fn previous_elements(arena: &crate::tree::Arena, id: NodeId) -> bool {
    std::iter::successors(arena[id].previous_sibling(), |&id| {
        arena[id].previous_sibling()
    })
    .any(|id| arena[id].kind == Kind::Element)
}

fn prefix(attribute: &Node) -> &str {
    if attribute.name.prefix.is_some() {
        &attribute.name.local
    } else {
        ""
    }
}
