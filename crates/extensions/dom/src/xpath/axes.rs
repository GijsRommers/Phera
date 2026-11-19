//! Traverse XPath axes in their predicate evaluation order.
//!
//! ```php
//! $xpath->query('preceding-sibling::*[1]', $node);
//! ```

use std::collections::{HashMap, HashSet};

use crate::tree::{Arena, Kind, NodeId};
use crate::xml::names::XMLNS;

use super::{Axis, Error, Eval, Step, Test};

impl Eval<'_> {
    pub(super) fn axis(&self, context: NodeId, step: &Step) -> Result<Vec<NodeId>, Error> {
        let arena = self.arena;
        let mut candidates: Vec<_> = match step.axis {
            Axis::Child => arena.children(context).collect(),
            Axis::Descendant => arena.descendants(context).collect(),
            Axis::DescendantOrSelf => std::iter::once(context)
                .chain(arena.descendants(context))
                .collect(),
            Axis::Parent => arena[context].parent().into_iter().collect(),
            Axis::Ancestor => arena.ancestors(context).collect(),
            Axis::AncestorOrSelf => std::iter::once(context)
                .chain(arena.ancestors(context))
                .collect(),
            Axis::FollowingSibling => following_siblings(arena, context).collect(),
            Axis::PrecedingSibling => preceding_siblings(arena, context).collect(),
            Axis::Attribute => arena[context]
                .attributes()
                .iter()
                .copied()
                .filter(|&id| arena[id].name.namespace.as_deref() != Some(XMLNS))
                .collect(),
            Axis::Namespace => arena.xpath_namespaces(context).to_vec(),
            Axis::SelfNode => vec![context],
            Axis::Following => side(arena, context, false),
            Axis::Preceding => side(arena, context, true),
        };
        let principal = match step.axis {
            Axis::Attribute => Kind::Attribute,
            Axis::Namespace => Kind::Namespace,
            _ => Kind::Element,
        };
        let qualified = match &step.test {
            Test::Named(name) => {
                let (namespace, name) = match name.split_once(':') {
                    Some((prefix, local)) => {
                        let uri = self.namespaces.get(prefix).ok_or(Error::Prefix)?;
                        (Some(uri.as_str()), local)
                    }
                    None => (None, name.as_str()),
                };
                Some((namespace, name))
            }
            _ => None,
        };
        candidates.retain(|&id| {
            let node = &arena[id];
            if node
                .parent()
                .is_some_and(|id| arena[id].kind == Kind::Entity)
            {
                return false;
            }
            match &step.test {
                Test::Node => !matches!(node.kind, Kind::Doctype | Kind::Entity | Kind::Notation),
                Test::Any => node.kind == principal,
                Test::Named(_) => qualified.is_some_and(|(namespace, local)| {
                    node.kind == principal
                        && node.name.namespace.as_deref() == namespace
                        && (local == "*" || node.name.local == local)
                }),
                Test::Text => matches!(node.kind, Kind::Text | Kind::Cdata),
                Test::Comment => node.kind == Kind::Comment,
                Test::ProcessingInstruction(target) => {
                    node.kind == Kind::ProcessingInstruction
                        && target
                            .as_ref()
                            .is_none_or(|target| *target == node.name.local)
                }
            }
        });
        Ok(candidates)
    }
}

fn following_siblings(arena: &Arena, context: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    std::iter::successors(arena[context].next_sibling(), |&id| {
        arena[id].next_sibling()
    })
}

fn preceding_siblings(arena: &Arena, context: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    std::iter::successors(arena[context].previous_sibling(), |&id| {
        arena[id].previous_sibling()
    })
}

fn side(arena: &Arena, context: NodeId, reverse: bool) -> Vec<NodeId> {
    let mut output = Vec::new();
    if !reverse
        && matches!(arena[context].kind, Kind::Attribute | Kind::Namespace)
        && let Some(parent) = arena[context].parent()
    {
        output.extend(arena.descendants(parent));
    }
    for entry in std::iter::once(context).chain(arena.ancestors(context)) {
        if reverse {
            for sibling in preceding_siblings(arena, entry) {
                let start = output.len();
                output.push(sibling);
                output.extend(arena.descendants(sibling));
                output[start..].reverse();
            }
        } else {
            for sibling in following_siblings(arena, entry) {
                output.push(sibling);
                output.extend(arena.descendants(sibling));
            }
        }
    }
    output
}

pub(super) fn document_order(arena: &Arena, mut nodes: Vec<NodeId>) -> Vec<NodeId> {
    if nodes.len() > 1 {
        let mut seen = HashSet::new();
        nodes.retain(|&id| seen.insert(id));
        let mut roots = Vec::new();
        let mut found = HashSet::new();
        for &id in &nodes {
            if !found.contains(&id) {
                let root = arena.ancestors(id).last().unwrap_or(id);
                // Mark the tree once so a path selecting a deep document stays linear.
                found.insert(root);
                for child in arena.descendants(root) {
                    found.insert(child);
                    found.extend(arena[child].attributes());
                    found.extend(arena.xpath_namespaces(child));
                }
                found.extend(arena[root].attributes());
                found.extend(arena.xpath_namespaces(root));
                roots.push(root);
            }
        }
        let mut indices = HashMap::new();
        let mut order = 0;
        for root in roots {
            for node in std::iter::once(root).chain(arena.descendants(root)) {
                if seen.contains(&node) {
                    indices.insert(node, order);
                    order += 1;
                }
                for &attribute in arena
                    .xpath_namespaces(node)
                    .iter()
                    .chain(arena[node].attributes())
                {
                    if seen.contains(&attribute) {
                        indices.insert(attribute, order);
                        order += 1;
                    }
                }
            }
        }
        nodes.sort_by_cached_key(|id| indices.get(id).copied().unwrap_or(usize::MAX));
    }
    nodes
}
