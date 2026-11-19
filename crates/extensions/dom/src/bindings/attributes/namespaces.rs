//! Give namespaced attributes a declaration in their element's scope.
//!
//! ```php
//! $node->setAttributeNS('urn:example', 'x:key', 'value');
//! echo $document->saveXML($node);
//! ```

use super::super::{namespace, node::namespace_scope};
use crate::tree::{Arena, Name, NodeId};
use crate::xml::names::{XML, XMLNS};

#[derive(Clone, Copy)]
pub(in crate::bindings) enum AttributeBinding {
    Direct,
    Node,
}

pub(in crate::bindings) fn bind(
    arena: &mut Arena,
    element: NodeId,
    name: &mut Name,
    binding: AttributeBinding,
) {
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
    let default_conflict = matches!(binding, AttributeBinding::Direct)
        && scope.get("").is_some_and(|bound| bound == uri);
    let require_prefix = requested.is_empty() && matches!(binding, AttributeBinding::Node);
    let base = if default_conflict || require_prefix || scope.contains_key(requested) {
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
    namespace::declare(arena, element, &prefix, uri);
    name.prefix = (!prefix.is_empty()).then_some(prefix);
}
