//! Walk the shared XML arena in reader order.
//!
//! ```php
//! $reader = XMLReader::fromString('<r><a/><b></b></r>');
//! while ($reader->read()) {
//!     echo $reader->name, ': ', $reader->nodeType, PHP_EOL;
//! }
//! ```

use phera_ext_dom::tree::{Arena, Kind, NodeId};

/// An attribute or end tag is a reader position on an existing arena node.
#[derive(Clone, Copy)]
pub enum Position {
    Before,
    Node {
        id: NodeId,
        depth: usize,
    },
    End {
        id: NodeId,
        depth: usize,
    },
    Attribute {
        id: NodeId,
        owner: NodeId,
        depth: usize,
    },
    After,
}

impl Position {
    pub fn id(self) -> Option<NodeId> {
        match self {
            Self::Node { id, .. } | Self::End { id, .. } | Self::Attribute { id, .. } => Some(id),
            _ => None,
        }
    }

    pub fn owner(self) -> Option<NodeId> {
        match self {
            Self::Attribute { owner, .. } => Some(owner),
            _ => self.id(),
        }
    }

    pub fn depth(self) -> usize {
        match self {
            Self::Node { depth, .. } | Self::End { depth, .. } => depth,
            Self::Attribute { depth, .. } => depth + 1,
            _ => 0,
        }
    }

    pub fn advance(self, arena: &Arena) -> Self {
        match self {
            Self::Before => arena[Arena::DOCUMENT]
                .first_child()
                .map_or(Self::After, |id| Self::Node { id, depth: 0 }),
            Self::Node { id, depth }
            | Self::Attribute {
                owner: id, depth, ..
            } => {
                let node = &arena[id];
                if node.kind == Kind::Element && !node.empty_element {
                    node.first_child()
                        .map_or(Self::End { id, depth }, |id| Self::Node {
                            id,
                            depth: depth + 1,
                        })
                } else {
                    sibling_or_end(arena, id, depth, true)
                }
            }
            Self::End { id, depth } => sibling_or_end(arena, id, depth, true),
            Self::After => Self::After,
        }
    }

    pub fn next(self, arena: &Arena, name: Option<&str>) -> Self {
        let (id, depth) = match self {
            Self::Node { id, depth } | Self::End { id, depth } => (id, depth),
            Self::Attribute { owner, depth, .. } => (owner, depth),
            Self::Before => return self.advance(arena),
            Self::After => return Self::After,
        };
        let mut candidate = sibling_or_end(arena, id, depth, name.is_none());
        while let Self::Node { id, depth } = candidate {
            if name.is_none_or(|name| arena[id].name.local == name) {
                return candidate;
            }
            candidate = sibling_or_end(arena, id, depth, false);
        }
        candidate
    }

    pub fn to_element(self) -> Option<Self> {
        match self {
            Self::Attribute {
                owner: id, depth, ..
            } => Some(Self::Node { id, depth }),
            _ => None,
        }
    }

    pub fn to_attribute(
        self,
        arena: &Arena,
        previous: bool,
        predicate: impl Fn(NodeId, usize) -> bool,
    ) -> Option<Self> {
        let (owner, depth, current) = match self {
            Self::Node { id, depth } => (id, depth, None),
            Self::Attribute { owner, id, depth } => (owner, depth, Some(id)),
            _ => return None,
        };
        let attributes = arena[owner].attributes();
        let start = if previous {
            current
                .and_then(|id| attributes.iter().position(|&attr| attr == id))
                .map_or(0, |index| index + 1)
        } else {
            0
        };
        let id = attributes
            .iter()
            .copied()
            .enumerate()
            .skip(start)
            .find_map(|(index, id)| predicate(id, index).then_some(id))?;
        Some(Self::Attribute { id, owner, depth })
    }
}

fn sibling_or_end(arena: &Arena, id: NodeId, depth: usize, use_end: bool) -> Position {
    if let Some(sibling) = arena[id].next_sibling() {
        return Position::Node { id: sibling, depth };
    }
    if use_end
        && let Some(parent) = arena[id].parent()
        && arena[parent].kind == Kind::Element
    {
        Position::End {
            id: parent,
            depth: depth.saturating_sub(1),
        }
    } else {
        Position::After
    }
}
