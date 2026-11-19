//! Node lists and named attribute maps are views of the current tree.
//!
//! ```php
//! $children = $root->childNodes;
//! $root->appendChild($document->createElement('new'));
//! echo $children->length;
//! ```

mod cursor;
mod dimensions;
mod registration;

pub(super) use registration::register;

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, Context, NativeError, NativePropertyMutation, NativePropertyRead, NativeResult,
    Object, PhpArray, PropertyPurpose, Value, args,
};

use crate::tree::{Arena, Kind, NodeId};
use crate::xml::names::XMLNS;

use super::{Location, NodeHandle, This, location, native, receiver};

#[derive(Clone)]
enum Selection {
    Children,
    Attributes,
    Declarations(Kind),
    Snapshot(Rc<[NodeId]>),
    Located(Rc<[Location]>),
    Tags {
        namespace: Option<Option<String>>,
        name: String,
    },
}

#[derive(Clone)]
struct Collection {
    root: Location,
    document_handle: Option<NodeHandle>,
    selection: Selection,
    _pins: Vec<Rc<()>>,
}

impl Collection {
    fn root(&self) -> Location {
        self.document_handle
            .as_ref()
            .map_or_else(|| self.root.clone(), NodeHandle::current)
    }

    fn ids(&self, arena: &Arena) -> Vec<NodeId> {
        let id = self.root.id;
        match &self.selection {
            Selection::Snapshot(nodes) => nodes.to_vec(),
            Selection::Located(nodes) => (0..nodes.len()).map(NodeId::from_index).collect(),
            Selection::Children if arena[id].kind == Kind::EntityReference => {
                std::iter::successors(arena.reference_declaration(id), |&id| {
                    arena[id]
                        .next_sibling()
                        .filter(|&id| arena[id].kind == Kind::Entity)
                })
                .collect()
            }
            Selection::Children => arena
                .children(id)
                .filter(|&id| arena[id].kind != Kind::Notation)
                .collect(),
            Selection::Declarations(kind) => arena
                .children(id)
                .filter(|&id| arena[id].kind == *kind)
                .collect(),
            Selection::Attributes => arena[id]
                .attributes()
                .iter()
                .copied()
                .filter(|&id| arena[id].name.namespace.as_deref() != Some(XMLNS))
                .collect(),
            Selection::Tags { namespace, name } => arena
                .content_descendants(id)
                .filter(|&id| {
                    let node = &arena[id];
                    node.kind == Kind::Element
                        && match namespace {
                            None => name == "*" || node.name.local == *name,
                            Some(namespace) => {
                                (name == "*" || node.name.local == *name)
                                    && (namespace.as_deref() == Some("*")
                                        || node.name.namespace == *namespace)
                            }
                        }
                })
                .collect(),
        }
    }

    fn wrap_node(&self, context: &mut dyn Context, id: Option<NodeId>) -> NativeResult {
        let root = self.root();
        if let Selection::Located(nodes) = &self.selection {
            return match id.and_then(|id| nodes.get(id.index())) {
                Some(node) => super::namespace::wrap(context, node.clone()),
                None => Ok(Value::Null),
            };
        }
        match id {
            Some(id) if matches!(self.selection, Selection::Snapshot(_)) => {
                super::namespace::wrap(context, root.at(id))
            }
            id => root.wrap_optional(context, id),
        }
    }

    fn snapshot(&self) -> Vec<NodeId> {
        self.ids(&self.root().shared.borrow().document.arena)
    }
}

pub(super) fn snapshot(
    context: &mut dyn Context,
    root: Location,
    nodes: Vec<NodeId>,
) -> NativeResult {
    wrap(
        context,
        root,
        Selection::Snapshot(nodes.into()),
        "DOMNodeList",
    )
}

pub(super) fn located(
    context: &mut dyn Context,
    root: Location,
    nodes: Vec<Location>,
) -> NativeResult {
    wrap(
        context,
        root,
        Selection::Located(nodes.into()),
        "DOMNodeList",
    )
}

pub(super) fn children(context: &mut dyn Context, root: Location) -> NativeResult {
    wrap(context, root, Selection::Children, "DOMNodeList")
}

pub(super) fn attributes(context: &mut dyn Context, root: Location) -> NativeResult {
    wrap(context, root, Selection::Attributes, "DOMNamedNodeMap")
}

pub(super) fn declarations(context: &mut dyn Context, root: Location, kind: Kind) -> NativeResult {
    wrap(
        context,
        root,
        Selection::Declarations(kind),
        "DOMNamedNodeMap",
    )
}

fn wrap(
    context: &mut dyn Context,
    root: Location,
    selection: Selection,
    class: &str,
) -> NativeResult {
    let value = context.create_object(class, PhpArray::new())?;
    if let Value::Object(object) = &value {
        let document_handle = if root.id == Arena::DOCUMENT
            && !matches!(selection, Selection::Snapshot(_) | Selection::Located(_))
        {
            root.shared
                .borrow()
                .wrappers
                .get(&root.id)
                .and_then(std::rc::Weak::upgrade)
                .and_then(|object| native(&object))
        } else {
            None
        };
        let pins = {
            let mut state = root.shared.borrow_mut();
            let mut pins = vec![state.pin(root.id)];
            if let Selection::Snapshot(nodes) = &selection {
                let arena = &state.document.arena;
                let owners: std::collections::HashSet<_> = nodes
                    .iter()
                    .filter(|&&id| arena[id].kind == Kind::Namespace)
                    .filter_map(|&id| arena[id].parent())
                    .collect();
                pins.extend(owners.into_iter().map(|id| state.pin(id)));
            }
            pins
        };
        *object.native.borrow_mut() = Some(Box::new(Collection {
            root,
            document_handle,
            selection,
            _pins: pins,
        }));
    }
    Ok(value)
}

pub(super) fn tags(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let name = path_parameter(
        object,
        "getElementsByTagName",
        arguments,
        0,
        "qualifiedName",
    )?;
    wrap(
        context,
        location(object)?,
        Selection::Tags {
            namespace: None,
            name,
        },
        "DOMNodeList",
    )
}

pub(super) fn tags_ns(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let object = receiver(this)?;
    let namespace = path_parameter(object, "getElementsByTagNameNS", arguments, 0, "namespace")?;
    let namespace = (!namespace.is_empty()).then_some(namespace);
    let name = path_parameter(object, "getElementsByTagNameNS", arguments, 1, "localName")?;
    wrap(
        context,
        location(object)?,
        Selection::Tags {
            namespace: Some(namespace),
            name,
        },
        "DOMNodeList",
    )
}

fn path_parameter(
    object: &Object,
    method: &str,
    arguments: &[Value],
    index: usize,
    name: &str,
) -> Result<String, NativeError> {
    let bytes = args::bytes(arguments, index);
    if bytes.contains(&0) {
        let class = if object.class.is_instance_of("DOMDocument") {
            "DOMDocument"
        } else {
            "DOMElement"
        };
        return Err(NativeError::value_error(format!(
            "{class}::{method}(): Argument #{} (${name}) must not contain any null bytes",
            index + 1,
        )));
    }
    Ok(String::from_utf8_lossy(bytes).into_owned())
}

fn collection(object: &Object) -> Result<Collection, NativeError> {
    native(object).ok_or_else(|| NativeError::error("DOM collection is uninitialized"))
}

fn count(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Int(count_hook(context, receiver(this)?)?))
}

fn count_hook(_: &mut dyn Context, object: &Object) -> Result<i64, NativeError> {
    Ok(collection(object)?.snapshot().len() as i64)
}

fn item(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let index = args::int(arguments, 0).unwrap_or_default();
    if !dimensions::valid_index(index) && dimensions::is_map(object) {
        return Err(NativeError::value_error(format!(
            "DOMNamedNodeMap::item(): Argument #1 ($index) {}",
            dimensions::INDEX_BOUNDS,
        )));
    }
    item_at(context, object, index)
}

fn item_at(context: &mut dyn Context, object: &Object, index: i64) -> NativeResult {
    let Some(collection): Option<Collection> = native(object) else {
        return Ok(Value::Null);
    };
    let id = usize::try_from(index)
        .ok()
        .and_then(|index| collection.snapshot().get(index).copied());
    collection.wrap_node(context, id)
}

fn named(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let name = String::from_utf8_lossy(args::bytes(arguments, 0));
    named_item(context, receiver(this)?, &name)
}

fn named_item(context: &mut dyn Context, object: &Object, name: &str) -> NativeResult {
    let collection = collection(object)?;
    let id = {
        let root = collection.root();
        let state = root.shared.borrow();
        let arena = &state.document.arena;
        collection
            .ids(arena)
            .into_iter()
            .find(|&id| arena[id].name.local == name)
    };
    collection.wrap_node(context, id)
}

fn named_ns(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let collection = collection(receiver(this)?)?;
    let namespace = args::string(arguments, 0)
        .filter(|value| !value.as_bytes().is_empty())
        .map(|value| String::from_utf8_lossy(value.as_bytes()));
    let name = String::from_utf8_lossy(args::bytes(arguments, 1));
    let id = {
        let root = collection.root();
        let state = root.shared.borrow();
        let arena = &state.document.arena;
        collection.ids(arena).into_iter().find(|&id| {
            arena[id].name.namespace.as_deref() == namespace.as_deref()
                && arena[id].name.local == name
        })
    };
    collection.wrap_node(context, id)
}

fn read(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    _: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    if name == "length" {
        Ok(Some(Value::Int(count_hook(context, object)?)))
    } else {
        Ok(None)
    }
}

fn mutate(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    _: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if name == "length" {
        return Err(NativeError::error(format!(
            "Cannot modify readonly property {}::$length",
            object.class.name()
        )));
    }
    Ok(false)
}

fn projection(
    context: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let mut table = object.to_array();
    if purpose == PropertyPurpose::Debug {
        table.insert(
            ArrayKey::str("length"),
            Value::Int(count_hook(context, object)?),
        );
    }
    Ok(table)
}
