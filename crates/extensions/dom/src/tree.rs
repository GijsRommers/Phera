//! Document-owned storage with stable indices and iterative traversal.
//!
//! ```php
//! $child = $parent->firstChild;
//! $parent->removeChild($child);
//! var_dump($child->parentNode); // NULL, but $child remains usable
//! ```

mod links;
mod walk;

use std::ops::{Index, IndexMut};

pub use links::LinkError;
pub use walk::{Children, Descendants};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NodeId(pub(crate) usize);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Element,
    Attribute,
    Text,
    Cdata,
    EntityReference,
    ProcessingInstruction,
    Comment,
    Document,
    Doctype,
    Fragment,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Name {
    pub local: String,
    pub prefix: Option<String>,
    pub namespace: Option<String>,
}

impl Name {
    pub fn new(local: impl Into<String>) -> Self {
        Self {
            local: local.into(),
            ..Self::default()
        }
    }

    pub fn qualified(&self) -> String {
        match &self.prefix {
            Some(prefix) => format!("{prefix}:{}", self.local),
            None => self.local.clone(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Doctype {
    pub public_id: String,
    pub system_id: String,
    pub internal_subset: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub kind: Kind,
    pub name: Name,
    pub value: String,
    pub line: usize,
    pub is_id: bool,
    pub doctype: Option<Doctype>,
    parent: Option<NodeId>,
    first: Option<NodeId>,
    last: Option<NodeId>,
    previous: Option<NodeId>,
    next: Option<NodeId>,
    attributes: Vec<NodeId>,
}

impl Node {
    pub fn new(kind: Kind, name: Name, value: impl Into<String>) -> Self {
        Self {
            kind,
            name,
            value: value.into(),
            line: 0,
            is_id: false,
            doctype: None,
            parent: None,
            first: None,
            last: None,
            previous: None,
            next: None,
            attributes: Vec::new(),
        }
    }

    pub fn parent(&self) -> Option<NodeId> {
        self.parent
    }

    pub fn first_child(&self) -> Option<NodeId> {
        self.first
    }

    pub fn last_child(&self) -> Option<NodeId> {
        self.last
    }

    pub fn previous_sibling(&self) -> Option<NodeId> {
        self.previous
    }

    pub fn next_sibling(&self) -> Option<NodeId> {
        self.next
    }

    pub fn attributes(&self) -> &[NodeId] {
        &self.attributes
    }

    fn detached_copy(&self) -> Self {
        Self {
            attributes: Vec::new(),
            parent: None,
            first: None,
            last: None,
            previous: None,
            next: None,
            ..self.clone()
        }
    }
}

#[derive(Debug)]
pub struct Arena {
    nodes: Vec<Node>,
}

impl Default for Arena {
    fn default() -> Self {
        Self {
            nodes: vec![Node::new(Kind::Document, Name::new("#document"), "")],
        }
    }
}

impl Arena {
    pub const DOCUMENT: NodeId = NodeId(0);

    pub fn allocate(&mut self, node: Node) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0)
    }

    /// Copy with destination indices, preserving attribute order.
    pub fn import(&mut self, source: &Self, node: NodeId, deep: bool) -> NodeId {
        let copy = self.copy_one(source, node);
        if deep {
            let mut pending = vec![(node, copy)];
            while let Some((original, parent)) = pending.pop() {
                for child in source.children(original) {
                    let copy = self.copy_one(source, child);
                    self.link(parent, copy, None);
                    pending.push((child, copy));
                }
            }
        }
        copy
    }

    fn copy_one(&mut self, source: &Self, node: NodeId) -> NodeId {
        let copy = self.allocate(source[node].detached_copy());
        for &attribute in source[node].attributes() {
            let attribute = self.allocate(source[attribute].detached_copy());
            self.link_attribute(copy, attribute);
        }
        copy
    }
}

impl Index<NodeId> for Arena {
    type Output = Node;

    fn index(&self, id: NodeId) -> &Node {
        &self.nodes[id.0]
    }
}

impl IndexMut<NodeId> for Arena {
    fn index_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id.0]
    }
}
