//! Rebind namespace names when a legacy DOM subtree is attached.
//!
//! ```php
//! $parent->appendChild($child);
//! echo $child->namespaceURI;
//! ```

use std::collections::{HashMap, HashSet};

use crate::tree::{Arena, Kind, Name, NodeId};
use crate::xml::names::{XML, XMLNS};

use super::super::node::namespace_scope;
use super::declare;

pub(in crate::bindings) fn reconcile(arena: &mut Arena, parent: NodeId, root: NodeId) {
    let reserve_default = arena[parent].kind == Kind::Fragment;
    reconcile_with_scope(arena, Some(parent), root, reserve_default);
}

pub(in crate::bindings) fn reconcile_detached(arena: &mut Arena, root: NodeId) {
    reconcile_with_scope(arena, None, root, true);
}

fn reconcile_with_scope(
    arena: &mut Arena,
    parent: Option<NodeId>,
    root: NodeId,
    reserve_default: bool,
) {
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
    let scope = parent
        .filter(|&id| arena[id].kind == Kind::Element)
        .map(|id| namespace_scope(arena, id));
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
        let mut prefix = if old.is_empty()
            && (reserve_default || arena[root].name.namespace.as_deref() != Some(&uri))
        {
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
    prune_redundant_descendants(arena, root, &inherited.bound);
}

pub(in crate::bindings) fn declare_import_bindings(arena: &mut Arena, root: NodeId) {
    if arena[root].kind != Kind::Element {
        return;
    }
    let mut declared: HashSet<_> = local_declarations(arena, root)
        .map(|(_, prefix, _)| prefix.to_owned())
        .collect();
    let used: Vec<Name> = std::iter::once(root)
        .chain(arena[root].attributes().iter().copied())
        .map(|id| arena[id].name.clone())
        .collect();
    for name in used {
        let Some(uri) = name.namespace.filter(|url| url != XML && url != XMLNS) else {
            continue;
        };
        let prefix = name.prefix.unwrap_or_default();
        if declared.insert(prefix.clone()) {
            declare(arena, root, &prefix, &uri);
        }
    }
}

fn prune_redundant_descendants(
    arena: &mut Arena,
    root: NodeId,
    inherited: &HashMap<String, String>,
) {
    let redundant: Vec<_> = arena
        .descendants(root)
        .flat_map(|id| local_declarations(arena, id))
        .filter(|(_, prefix, url)| inherited.get(*prefix).is_some_and(|bound| bound == url))
        .filter_map(|(id, prefix, url)| {
            let element = arena[id].parent()?;
            let ancestor = arena[element].parent()?;
            namespace_scope(arena, ancestor)
                .find(|(candidate, _)| candidate == prefix)
                .filter(|(_, bound)| bound == url)
                .map(|_| id)
        })
        .collect();
    for id in redundant {
        arena.detach(id);
    }
}

fn local_declarations(arena: &Arena, id: NodeId) -> impl Iterator<Item = (NodeId, &str, &str)> {
    arena[id].attributes().iter().filter_map(|&attribute| {
        let entry = &arena[attribute];
        (entry.name.namespace.as_deref() == Some(XMLNS)).then(|| {
            let prefix = if entry.name.prefix.is_some() {
                entry.name.local.as_str()
            } else {
                ""
            };
            (attribute, prefix, entry.value.as_str())
        })
    })
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
