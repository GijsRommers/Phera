//! Give namespaced attributes a declaration in their element's scope.
//!
//! ```php
//! $node->setAttributeNS('urn:example', 'x:key', 'value');
//! echo $document->saveXML($node);
//! ```

use super::super::node::namespace_scope;
use crate::tree::{Arena, Kind, Name, Node, NodeId};
use crate::xml::names::{self, XML, XMLNS};

pub(super) fn bind(arena: &mut Arena, element: NodeId, name: &mut Name) {
    let Some(uri) = name.namespace.as_deref() else {
        return;
    };
    if uri == XML || uri == XMLNS {
        return;
    }
    let mut scope = std::collections::HashMap::new();
    let mut preferred = None;
    for (prefix, bound) in namespace_scope(arena, element) {
        if let std::collections::hash_map::Entry::Vacant(entry) = scope.entry(prefix.clone()) {
            if bound == uri && !prefix.is_empty() && preferred.is_none() {
                preferred = Some(prefix);
            }
            entry.insert(bound);
        }
    }
    if let Some(prefix) = preferred {
        name.prefix = Some(prefix);
        return;
    }
    let requested = name.prefix.as_deref().unwrap_or("");
    let default_bound = scope.get("").is_some_and(|bound| bound == uri);
    if requested.is_empty() && default_bound {
        return;
    }
    let base = if default_bound || (requested.is_empty() && scope.contains_key("")) {
        "default"
    } else {
        requested
    };
    let mut prefix = base.to_owned();
    for index in 1..=scope.len() {
        if !scope.contains_key(&prefix) {
            break;
        }
        prefix = format!("{base}{index}");
    }
    let qualified = if prefix.is_empty() {
        "xmlns".into()
    } else {
        format!("xmlns:{prefix}")
    };
    let declaration = arena.allocate(Node::new(
        Kind::Attribute,
        names::qualified(&qualified, Some(XMLNS.into())),
        uri,
    ));
    let _ = arena.set_attribute(element, declaration, None);
    name.prefix = (!prefix.is_empty()).then_some(prefix);
}
