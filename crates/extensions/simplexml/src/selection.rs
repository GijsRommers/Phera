//! A SimpleXMLElement can represent a single node or a live selection.
//!
//! ```php
//! $items = $xml->item;
//! echo $items->count();
//! echo $items[0]->getName();
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use phera_ext_dom::shared::SharedNode;
use phera_ext_dom::tree::{Arena, Kind, Name, NodeId};
use phera_extension_api::{Context, NativeError, NativeResult, Object, Value};

pub const CLASS: &str = "SimpleXMLElement";
const XMLNS: &str = "http://www.w3.org/2000/xmlns/";

#[derive(Clone, Default)]
pub enum Filter {
    #[default]
    Default,
    Prefix(String),
    Namespace(String),
}

impl Filter {
    pub fn new(value: Option<&str>, is_prefix: bool) -> Self {
        match value.filter(|value| !value.is_empty()) {
            Some(value) if is_prefix => Self::Prefix(value.to_owned()),
            Some(value) => Self::Namespace(value.to_owned()),
            None => Self::Default,
        }
    }

    pub fn matches(&self, name: &Name) -> bool {
        match self {
            Self::Default => name.prefix.is_none(),
            Self::Prefix(prefix) => name.prefix.as_ref() == Some(prefix),
            Self::Namespace(namespace) => name.namespace.as_ref() == Some(namespace),
        }
    }
}

#[derive(Clone)]
pub enum Selected {
    Node,
    Children(Option<String>),
    Attributes(Option<String>),
}

#[derive(Clone)]
pub struct Selection {
    pub anchor: SharedNode,
    pub kind: Selected,
    pub filter: Filter,
}

impl Selection {
    pub fn node(anchor: SharedNode, filter: Filter) -> Self {
        Self {
            anchor,
            kind: Selected::Node,
            filter,
        }
    }

    pub fn nodes_in(&self, arena: &Arena) -> Vec<NodeId> {
        let owner = self.anchor.id();
        match &self.kind {
            Selected::Node => vec![owner],
            Selected::Children(name) => children(arena, owner, &self.filter, name.as_deref()),
            Selected::Attributes(name) => attributes(arena, owner, &self.filter, name.as_deref()),
        }
    }

    pub fn first(&self) -> Option<SharedNode> {
        let id = self
            .anchor
            .document(|doc| self.nodes_in(&doc.arena).first().copied())?;
        self.anchor.select(id)
    }

    pub fn items(&self) -> Vec<NodeId> {
        self.anchor.document(|document| {
            let arena = &document.arena;
            match self.kind {
                Selected::Node => children(arena, self.anchor.id(), &self.filter, None),
                _ => self.nodes_in(arena),
            }
        })
    }

    pub fn next_item(&self, previous: Option<&SharedNode>) -> Option<SharedNode> {
        let id = self.anchor.document(|doc| {
            let arena = &doc.arena;
            let owner = self.anchor.id();
            let previous = previous.map(SharedNode::id);
            match &self.kind {
                Selected::Attributes(expected) => {
                    let attributes = arena[owner].attributes();
                    let start = match previous {
                        Some(id) => attributes.iter().position(|&attr| attr == id)? + 1,
                        None => 0,
                    };
                    attributes[start..].iter().copied().find(|&attr| {
                        let name = &arena[attr].name;
                        name.namespace.as_deref() != Some(XMLNS)
                            && self.filter.matches(name)
                            && expected
                                .as_ref()
                                .is_none_or(|expected| name.local == *expected)
                    })
                }
                kind => {
                    let expected = match kind {
                        Selected::Children(name) => name.as_deref(),
                        _ => None,
                    };
                    let mut next = match previous {
                        Some(id) if arena[id].parent() == Some(owner) => arena[id].next_sibling(),
                        Some(_) => None,
                        None => arena[owner].first_child(),
                    };
                    while let Some(id) = next {
                        let entry = &arena[id];
                        if entry.kind == Kind::Element
                            && self.filter.matches(&entry.name)
                            && expected.is_none_or(|name| entry.name.local == name)
                        {
                            return Some(id);
                        }
                        next = entry.next_sibling();
                    }
                    None
                }
            }
        })?;
        self.anchor.select(id)
    }

    pub fn name(&self) -> String {
        self.first()
            .map(|node| {
                node.document(|doc| {
                    let entry = &doc.arena[node.id()];
                    if entry.kind == Kind::Comment {
                        "comment".to_owned()
                    } else {
                        entry.name.local.clone()
                    }
                })
            })
            .unwrap_or_default()
    }

    pub fn value(&self) -> String {
        self.first().map_or_else(String::new, |node| {
            node.document(|doc| value(&doc.arena, node.id()))
        })
    }

    pub fn attribute_owner(&self) -> Option<SharedNode> {
        match self.kind {
            Selected::Attributes(_) => Some(self.anchor.clone()),
            Selected::Children(None) => None,
            _ => self.first(),
        }
    }

    pub fn for_property(&self, name: String) -> Option<Self> {
        let owner = match self.kind {
            Selected::Children(None) => self.anchor.clone(),
            _ => self.first()?,
        };
        if !owner.document(|doc| doc.arena[owner.id()].kind == Kind::Element) {
            return None;
        }
        Some(Self {
            anchor: owner,
            kind: Selected::Children(Some(name)),
            filter: self.filter.clone(),
        })
    }

    pub fn is_truthy(&self) -> bool {
        match self.kind {
            Selected::Node => self.anchor.document(|doc| {
                let node = &doc.arena[self.anchor.id()];
                node.kind != Kind::Element
                    || !attributes(&doc.arena, self.anchor.id(), &self.filter, None).is_empty()
                    || doc.arena.children(self.anchor.id()).any(|child| {
                        let child = &doc.arena[child];
                        match child.kind {
                            Kind::Element => self.filter.matches(&child.name),
                            Kind::Cdata | Kind::EntityReference => false,
                            _ => true,
                        }
                    })
            }),
            _ => self.first().is_some(),
        }
    }
}

pub fn children(arena: &Arena, owner: NodeId, filter: &Filter, name: Option<&str>) -> Vec<NodeId> {
    arena
        .children(owner)
        .filter(|&child| {
            let node = &arena[child];
            node.kind == Kind::Element
                && filter.matches(&node.name)
                && name.is_none_or(|name| node.name.local == name)
        })
        .collect()
}

pub fn attributes(
    arena: &Arena,
    owner: NodeId,
    filter: &Filter,
    name: Option<&str>,
) -> Vec<NodeId> {
    arena[owner]
        .attributes()
        .iter()
        .copied()
        .filter(|&child| {
            let node = &arena[child];
            node.name.namespace.as_deref() != Some(XMLNS)
                && filter.matches(&node.name)
                && name.is_none_or(|name| node.name.local == name)
        })
        .collect()
}

pub fn value(arena: &Arena, id: NodeId) -> String {
    if arena[id].kind != Kind::Element {
        return arena[id].value.clone();
    }
    let mut result = String::new();
    for child in arena.children(id) {
        let node = &arena[child];
        if matches!(node.kind, Kind::Text | Kind::Cdata | Kind::EntityReference) {
            result.push_str(&node.value);
        }
    }
    result
}

pub struct State {
    pub selection: Selection,
    pub materialize_on_mutation: bool,
    pub namespaces: RefCell<HashMap<String, String>>,
    pub cursor: RefCell<Option<SharedNode>>,
}

pub fn data(object: &Object) -> Result<Rc<State>, NativeError> {
    object
        .native
        .borrow()
        .as_ref()
        .and_then(|native| native.downcast_ref::<Rc<State>>())
        .cloned()
        .ok_or_else(|| NativeError::error("SimpleXMLElement is not properly initialized"))
}

pub fn install(object: &Object, selection: Selection) {
    install_selection(object, selection, false);
}

fn install_selection(object: &Object, selection: Selection, materialize_on_mutation: bool) {
    *object.native.borrow_mut() = Some(Box::new(Rc::new(State {
        selection,
        materialize_on_mutation,
        namespaces: RefCell::default(),
        cursor: RefCell::default(),
    })));
}

pub fn wrap(context: &mut dyn Context, class: &str, selection: Selection) -> NativeResult {
    wrap_selection(context, class, selection, false)
}

pub fn wrap_for_mutation(
    context: &mut dyn Context,
    class: &str,
    selection: Selection,
) -> NativeResult {
    wrap_selection(context, class, selection, true)
}

fn wrap_selection(
    context: &mut dyn Context,
    class: &str,
    selection: Selection,
    materialize_on_mutation: bool,
) -> NativeResult {
    let value = context.create_unserialized_object(class)?;
    let Value::Object(ref object) = value else {
        return Err(NativeError::error("Could not create SimpleXMLElement"));
    };
    install_selection(object, selection, materialize_on_mutation);
    Ok(value)
}
