//! Rebind namespace names when a legacy DOM subtree is attached.
//!
//! ```php
//! $parent->appendChild($child);
//! echo $child->namespaceURI;
//! ```

use std::collections::{HashMap, HashSet};

use crate::tree::{Arena, Kind, NodeId};
use crate::xml::names::{XML, XMLNS};

use super::super::node::namespace_scope;
use super::declare;

pub(in crate::bindings) fn reconcile(arena: &mut Arena, parent: NodeId, root: NodeId) {
    if arena[root].kind != Kind::Element {
        return;
    }
    let names: Vec<_> = std::iter::once(root)
        .chain(arena.descendants(root))
        .filter(|&id| arena[id].kind == Kind::Element)
        .flat_map(|id| std::iter::once(id).chain(arena[id].attributes().iter().copied()))
        .filter(|&id| {
            arena[id]
                .name
                .namespace
                .as_deref()
                .is_some_and(|uri| uri != XML && uri != XMLNS)
        })
        .collect();
    let declared: Vec<_> = arena[root]
        .attributes()
        .iter()
        .copied()
        .filter(|&id| arena[id].name.namespace.as_deref() == Some(XMLNS))
        .collect();
    if names.is_empty() && declared.is_empty() {
        return;
    }
    let scope = (arena[parent].kind == Kind::Element).then(|| namespace_scope(arena, parent));
    let mut inherited = Bindings::new(scope.into_iter().flatten());
    let mut local = HashMap::new();
    for id in declared {
        let entry = &arena[id];
        let prefix = if entry.name.prefix.is_some() {
            entry.name.local.as_str()
        } else {
            ""
        };
        let uri = entry.value.as_str();
        let reuses_default = prefix.is_empty()
            && !uri.is_empty()
            && arena[root].name.prefix.is_none()
            && arena[root].name.namespace.as_deref() == Some(uri)
            && inherited.preferred.contains_key(uri);
        if reuses_default
            || inherited
                .bound
                .get(prefix)
                .is_some_and(|bound| bound == uri)
        {
            arena.detach(id);
        } else {
            local.insert(prefix.to_owned(), uri.to_owned());
        }
    }
    for (prefix, uri) in &local {
        if inherited
            .bound
            .get(prefix)
            .is_some_and(|bound| bound != uri)
        {
            inherited
                .preferred
                .retain(|_, preferred| preferred != prefix);
        }
    }
    for id in names {
        let old = arena[id].name.prefix.as_deref().unwrap_or("");
        let Some(uri) = arena[id].name.namespace.as_deref() else {
            continue;
        };
        if local.get(old).is_some_and(|bound| bound == uri)
            || (!local.contains_key(old)
                && inherited.bound.get(old).is_some_and(|bound| bound == uri))
        {
            continue;
        }
        if let Some(prefix) = inherited.preferred.get(uri)
            && (!prefix.is_empty() || arena[id].kind == Kind::Element)
        {
            arena[id].name.prefix = (!prefix.is_empty()).then(|| prefix.clone());
            continue;
        }
        let uri = uri.to_owned();
        let base = if old.is_empty() { "default" } else { old };
        // A descendant's default namespace must not change the root's name.
        let mut prefix = if old.is_empty() && arena[root].name.namespace.as_deref() != Some(&uri) {
            base.to_owned()
        } else {
            old.to_owned()
        };
        let mut index = 1;
        while inherited.bound.contains_key(&prefix) || local.contains_key(&prefix) {
            prefix = format!("{base}{index}");
            index += 1;
        }
        declare(arena, root, &prefix, &uri);
        local.insert(prefix.clone(), uri.clone());
        inherited.preferred.insert(uri, prefix.clone());
        arena[id].name.prefix = (!prefix.is_empty()).then_some(prefix);
    }
}

struct Bindings {
    bound: HashMap<String, String>,
    preferred: HashMap<String, String>,
}

impl Bindings {
    fn new(scope: impl Iterator<Item = (String, String)>) -> Self {
        let mut seen = HashSet::new();
        let mut bound = HashMap::new();
        let mut preferred = HashMap::new();
        for (prefix, uri) in scope {
            if seen.insert(prefix.clone()) {
                bound.insert(prefix.clone(), uri.clone());
                preferred.entry(uri).or_insert(prefix);
            }
        }
        Self { bound, preferred }
    }
}
