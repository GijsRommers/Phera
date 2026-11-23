//! `$map[$key] = $key` cannot by itself make `$key` reachable.
//!
//! A marked map and key activate their entry. Register the condition at
//! both endpoints so the last endpoint reached can wake the value.

use std::{collections::HashMap, hash::BuildHasherDefault};

use super::AddressHasher;

pub(super) struct WeakEdge {
    pub(super) key: usize,
    /// The map's identity, which is its allocation address.
    pub(super) owner: usize,
    /// Leaf objects are counted separately if their last owner disappears.
    pub(super) child: Option<usize>,
}

struct Dependency {
    other: Option<usize>,
    child: usize,
    next: usize,
}

/// Flat lists indexed by the value that made a dependency eligible.
#[derive(Default)]
pub(super) struct Dependencies {
    heads: Vec<usize>,
    links: Vec<Dependency>,
}

impl Dependencies {
    pub(super) fn new(
        nodes: usize,
        edges: &[WeakEdge],
        index_of: &HashMap<usize, usize, BuildHasherDefault<AddressHasher>>,
    ) -> Self {
        let mut dependencies = Self::default();
        for edge in edges {
            let Some(child) = edge.child else { continue };
            if dependencies.heads.is_empty() {
                dependencies.heads.resize(nodes, usize::MAX);
            }
            let owner = index_of.get(&edge.owner).copied();
            dependencies.register(edge.key, owner, child);
            if let Some(owner) = owner.filter(|owner| *owner != edge.key) {
                dependencies.register(owner, Some(edge.key), child);
            }
        }
        dependencies
    }

    fn register(&mut self, endpoint: usize, other: Option<usize>, child: usize) {
        let next = std::mem::replace(&mut self.heads[endpoint], self.links.len());
        self.links.push(Dependency { other, child, next });
    }

    pub(super) fn visit(&self, endpoint: usize, mut visit: impl FnMut(Option<usize>, usize)) {
        let Some(&first) = self.heads.get(endpoint) else {
            return;
        };
        let mut head = first;
        while let Some(dependency) = self.links.get(head) {
            visit(dependency.other, dependency.child);
            head = dependency.next;
        }
    }
}
