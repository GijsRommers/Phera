//! Preserve namespace bindings when a declaration is removed by name.
//!
//! ```php
//! $root->removeAttributeNS('urn:example', 'p');
//! ```

use crate::tree::{Arena, Kind, NodeId};
use crate::xml::names::XMLNS;

pub(in crate::bindings) fn restore_used(arena: &mut Arena, root: NodeId, declaration: NodeId) {
    let node = &arena[declaration];
    let prefix = node.name.prefix.as_ref().map(|_| node.name.local.clone());
    let prefix = prefix.as_deref().unwrap_or("");
    let uri = node.value.clone();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if id != root
            && let Some(sibling) = arena[id].next_sibling()
        {
            pending.push(sibling);
        }
        if arena[id].kind != Kind::Element || local_declaration(arena, id, prefix).is_some() {
            continue;
        }
        let uses_binding = std::iter::once(id)
            .chain(arena[id].attributes().iter().copied())
            .any(|candidate| {
                let name = &arena[candidate].name;
                name.namespace.as_deref() == Some(&uri)
                    && name.prefix.as_deref().unwrap_or("") == prefix
            });
        if uses_binding {
            super::declare(arena, id, prefix, &uri);
        } else if let Some(child) = arena[id].first_child() {
            pending.push(child);
        }
    }
}

pub(in crate::bindings) fn remove_by_uri(arena: &mut Arena, root: NodeId, prefix: &str, uri: &str) {
    let declaration = local_declaration(arena, root, prefix).filter(|&id| arena[id].value == uri);
    let Some(declaration) = declaration else {
        return;
    };
    arena.detach(declaration);
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if id != root {
            if let Some(next) = arena[id].next_sibling() {
                pending.push(next);
            }
            if local_declaration(arena, id, prefix).is_some() {
                continue;
            }
        }
        if arena[id].kind != Kind::Element {
            continue;
        }
        if let Some(first) = arena[id].first_child() {
            pending.push(first);
        }
        clear_name(arena, id, prefix, uri);
        for index in 0..arena[id].attributes().len() {
            let attribute = arena[id].attributes()[index];
            clear_name(arena, attribute, prefix, uri);
        }
    }
}

fn local_declaration(arena: &Arena, root: NodeId, prefix: &str) -> Option<NodeId> {
    arena[root].attributes().iter().copied().find(|&id| {
        let entry = &arena[id];
        entry.name.namespace.as_deref() == Some(XMLNS)
            && if prefix.is_empty() {
                entry.name.prefix.is_none()
            } else {
                entry.name.prefix.is_some() && entry.name.local == prefix
            }
    })
}

fn clear_name(arena: &mut Arena, id: NodeId, prefix: &str, uri: &str) {
    let name = &mut arena[id].name;
    if name.namespace.as_deref() == Some(uri) && name.prefix.as_deref().unwrap_or("") == prefix {
        name.prefix = None;
        name.namespace = None;
    }
}
