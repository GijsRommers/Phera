//! Model the XPath namespace axis without adding children to the document tree.
//!
//! ```php
//! $namespaces = $xpath->query('/*/namespace::*');
//! ```

use std::collections::HashMap;

use super::{Arena, Kind, Name, Node, NodeId};
use crate::xml::names::{XML, XMLNS};

#[derive(Clone, Debug, Default)]
pub(super) struct XPathNamespaces {
    revision: Option<u64>,
    by_owner: HashMap<NodeId, Vec<NodeId>>,
    entries: HashMap<(NodeId, String, String), NodeId>,
}

enum Event {
    Enter(NodeId),
    Restore(Vec<(String, String)>),
}

impl Arena {
    pub fn prepare_xpath_namespaces(&mut self, context: NodeId) {
        let revision = self.revision();
        let mut namespaces = self.xpath_namespaces.take().unwrap_or_default();
        if namespaces.revision != Some(revision) {
            namespaces.by_owner.clear();
            namespaces.revision = Some(revision);
        }
        let root = self.ancestors(context).last().unwrap_or(context);
        for root in [Self::DOCUMENT, root] {
            if !namespaces.by_owner.contains_key(&root) {
                namespaces.capture(self, root);
            }
        }
        self.xpath_namespaces = Some(namespaces);
    }

    pub(crate) fn xpath_namespaces(&self, owner: NodeId) -> &[NodeId] {
        self.xpath_namespaces
            .as_ref()
            .filter(|state| state.revision == Some(self.revision))
            .and_then(|state| state.by_owner.get(&owner))
            .map_or(&[], Vec::as_slice)
    }
}

impl XPathNamespaces {
    fn capture(&mut self, arena: &mut Arena, root: NodeId) {
        let mut scope = vec![("xml".to_owned(), XML.to_owned())];
        let mut events = vec![Event::Enter(root)];
        self.by_owner.entry(root).or_default();
        while let Some(event) = events.pop() {
            match event {
                Event::Enter(id) => {
                    if let Some(next) = arena[id].next_sibling().filter(|_| id != root) {
                        events.push(Event::Enter(next));
                    }
                    if arena[id].kind == Kind::Element {
                        let declared = declarations(arena, id);
                        if !declared.is_empty() {
                            events.push(Event::Restore(scope.clone()));
                            scope.retain(|(name, _)| {
                                !declared.iter().any(|(prefix, _)| prefix == name)
                            });
                            scope.extend(declared);
                        }
                        let nodes = scope
                            .iter()
                            .map(|(prefix, uri)| self.node(arena, id, prefix, uri))
                            .collect();
                        self.by_owner.insert(id, nodes);
                    }
                    if let Some(child) = arena[id].first_child() {
                        events.push(Event::Enter(child));
                    }
                }
                Event::Restore(previous) => scope = previous,
            }
        }
    }

    fn node(&mut self, arena: &mut Arena, owner: NodeId, prefix: &str, uri: &str) -> NodeId {
        let key = (owner, prefix.to_owned(), uri.to_owned());
        *self.entries.entry(key).or_insert_with(|| {
            let mut node = Node::new(Kind::Namespace, Name::new(prefix), uri);
            node.parent = Some(owner);
            arena.allocate(node)
        })
    }
}

fn declarations(arena: &Arena, owner: NodeId) -> Vec<(String, String)> {
    arena[owner]
        .attributes()
        .iter()
        .rev()
        .filter_map(|&id| {
            let node = &arena[id];
            if node.name.namespace.as_deref() != Some(XMLNS) {
                return None;
            }
            let local = if node.name.prefix.is_some() {
                &node.name.local
            } else {
                ""
            };
            (local != "xml").then(|| (local.to_owned(), node.value.clone()))
        })
        .collect()
}
