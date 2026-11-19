//! Node lists and named attribute maps are views of the current tree.
//!
//! ```php
//! $children = $root->childNodes;
//! $root->appendChild($document->createElement('new'));
//! echo $children->length;
//! ```

mod cursor;
mod dimensions;

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, Context, MethodEntry, NativeDimensions, NativeError,
    NativePropertyAccess, NativePropertyMutation, NativePropertyRead, NativeResult, Object, Param,
    ParamType, PhpArray, PropertyPurpose, Registry, Value, args,
};

use crate::tree::{Arena, Kind, NodeId};
use crate::xml::names::XMLNS;

use super::{Location, This, location, native, receiver};

#[derive(Clone)]
enum Selection {
    Children,
    Attributes,
    Declarations(Kind),
    Snapshot(Rc<[NodeId]>),
    Tags {
        namespace: Option<Option<String>>,
        name: String,
    },
}

#[derive(Clone)]
struct Collection {
    root: Location,
    selection: Selection,
    _pin: Rc<()>,
}

impl Collection {
    fn ids(&self, arena: &Arena) -> Vec<NodeId> {
        let id = self.root.id;
        match &self.selection {
            Selection::Snapshot(nodes) => nodes.to_vec(),
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
                .descendants(id)
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
        match id {
            Some(id) if matches!(self.selection, Selection::Snapshot(_)) => {
                super::namespace::wrap(context, self.root.at(id))
            }
            id => self.root.wrap_optional(context, id),
        }
    }

    fn snapshot(&self) -> Vec<NodeId> {
        self.ids(&self.root.shared.borrow().document.arena)
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
        let pin = root.shared.borrow_mut().pin(root.id);
        *object.native.borrow_mut() = Some(Box::new(Collection {
            root,
            selection,
            _pin: pin,
        }));
    }
    Ok(value)
}

pub(super) fn tags(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let name = String::from_utf8_lossy(args::bytes(arguments, 0)).into_owned();
    wrap(
        context,
        location(receiver(this)?)?,
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
    let namespace = args::string(arguments, 0)
        .filter(|value| !value.as_bytes().is_empty())
        .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned());
    let name = String::from_utf8_lossy(args::bytes(arguments, 1)).into_owned();
    wrap(
        context,
        location(receiver(this)?)?,
        Selection::Tags {
            namespace: Some(namespace),
            name,
        },
        "DOMNodeList",
    )
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
    let collection = collection(object)?;
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
        let state = collection.root.shared.borrow();
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
        let state = collection.root.shared.borrow();
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

const HOOKS: ClassHooks = ClassHooks {
    iterator: Some(cursor::iterator),
    properties: Some(projection),
    property_access: Some(&NativePropertyAccess { read, mutate }),
    count: Some(count_hook),
    dimensions: Some(NativeDimensions {
        fallback_if_overridden: &[],
        get: dimensions::get,
        exists: dimensions::exists,
        set: dimensions::set,
        unset: dimensions::unset,
        reference: None,
        bind: None,
        direct: None,
    }),
    ..ClassHooks::EMPTY
};

pub(super) fn register(registry: &mut Registry) {
    const ITEM: MethodEntry = MethodEntry::public(
        "item",
        const { &[Param::new("index", ParamType::Int)] },
        item,
    );
    const COUNT: MethodEntry = MethodEntry::public("count", &[], count).tentative_return("int");
    const ITERATOR: MethodEntry =
        MethodEntry::public("getIterator", &[], cursor::get_iterator).returning("Iterator");
    registry.class(ClassEntry {
        interfaces: &["IteratorAggregate", "Countable"],
        methods: const { &[ITEM, COUNT, ITERATOR] },
        hooks: Some(&HOOKS),
        ..ClassEntry::class("DOMNodeList")
    });
    registry.class(ClassEntry {
        interfaces: &["IteratorAggregate", "Countable"],
        methods: const {
            const {
                &[
                    ITEM.tentative_return("?DOMNode"),
                    COUNT,
                    ITERATOR,
                    MethodEntry::public(
                        "getNamedItem",
                        const { &[Param::new("qualifiedName", ParamType::String)] },
                        named,
                    )
                    .tentative_return("?DOMNode"),
                    MethodEntry::public(
                        "getNamedItemNS",
                        const {
                            &[
                                Param::new("namespace", ParamType::String).nullable(),
                                Param::new("localName", ParamType::String),
                            ]
                        },
                        named_ns,
                    )
                    .tentative_return("?DOMNode"),
                ]
            }
        },
        hooks: Some(&HOOKS),
        ..ClassEntry::class("DOMNamedNodeMap")
    });
}
