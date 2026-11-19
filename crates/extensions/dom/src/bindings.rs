//! Connect PHP objects and native methods to the document arena.
//!
//! ```php
//! $element = $document->documentElement;
//! assert($element === $document->documentElement);
//! ```

mod attributes;
mod characters;
mod collection;
mod detached;
mod document;
mod factory;
mod libxml;
mod mutation;
mod namespace;
mod node;
mod properties;
mod xpath;

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use phera_extension_api::{
    ClassEntry, ConstantValue, Context, Extension, NativeError, NativeResult, Object, PhpArray,
    PropertyEntry, Registry, Value, Visibility,
};

use crate::document::Document;
use crate::tree::{Arena, Kind, NodeId};

pub struct Dom;
pub struct Libxml;

impl Extension for Dom {
    fn name(&self) -> &'static str {
        "dom"
    }

    fn module_version(&self) -> Option<&'static str> {
        Some("20031129")
    }

    fn register(&self, registry: &mut Registry) {
        registry.class(ClassEntry {
            parent: Some("Exception"),
            kind: phera_extension_api::ClassKind::FinalClass,
            properties: const {
                &[PropertyEntry::new(
                    "code",
                    ConstantValue::Int(0),
                    Visibility::Public,
                )]
            },
            ..ClassEntry::class("DOMException")
        });
        libxml::register_dom_constants(registry);
        node::register(registry);
        document::register(registry);
        namespace::register(registry);
        factory::register(registry);
        collection::register(registry);
        xpath::register(registry);
    }
}

impl Extension for Libxml {
    fn name(&self) -> &'static str {
        "libxml"
    }
    fn register(&self, registry: &mut Registry) {
        libxml::register(registry);
    }
}

type This<'a> = Option<&'a Rc<Object>>;
type Shared = Rc<RefCell<DomState>>;

struct DomState {
    document: Document,
    owned: bool,
    wrappers: HashMap<NodeId, Weak<Object>>,
    node_classes: HashMap<String, String>,
    detached: detached::Detached,
}

#[derive(Clone)]
struct Location {
    shared: Shared,
    id: NodeId,
}

#[derive(Clone)]
struct NodeHandle(Rc<RefCell<Location>>);

impl NodeHandle {
    fn current(&self) -> Location {
        self.0.borrow().clone()
    }
}

impl Location {
    fn new(document: Document, owned: bool, id: NodeId) -> Self {
        Self {
            shared: Rc::new(RefCell::new(DomState {
                document,
                owned,
                wrappers: HashMap::new(),
                node_classes: HashMap::new(),
                detached: detached::Detached::default(),
            })),
            id,
        }
    }

    fn same_document(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.shared, &other.shared)
    }

    fn at(&self, id: NodeId) -> Self {
        Self {
            shared: self.shared.clone(),
            id,
        }
    }

    fn bind(self, object: &Rc<Object>) {
        let handle = native::<NodeHandle>(object);
        if let Some(handle) = &handle {
            let previous = handle.current();
            if (!previous.same_document(&self) || previous.id != self.id)
                && let Ok(mut state) = previous.shared.try_borrow_mut()
            {
                state.wrappers.remove(&previous.id);
            }
        }
        self.shared
            .borrow_mut()
            .wrappers
            .insert(self.id, Rc::downgrade(object));
        if let Some(handle) = handle {
            *handle.0.borrow_mut() = self;
        } else {
            *object.native.borrow_mut() = Some(Box::new(NodeHandle(Rc::new(RefCell::new(self)))));
        }
    }

    fn wrap(self, context: &mut dyn Context) -> NativeResult {
        let class = {
            let state = self.shared.borrow();
            if let Some(object) = state.wrappers.get(&self.id).and_then(Weak::upgrade) {
                return Ok(Value::Object(object));
            }
            let class = match state.document.arena[self.id].kind {
                Kind::Element => "DOMElement",
                Kind::Attribute => "DOMAttr",
                Kind::Namespace => "DOMNameSpaceNode",
                Kind::Text => "DOMText",
                Kind::Cdata => "DOMCdataSection",
                Kind::Comment => "DOMComment",
                Kind::ProcessingInstruction => "DOMProcessingInstruction",
                Kind::Document => "DOMDocument",
                Kind::Doctype => "DOMDocumentType",
                Kind::Fragment => "DOMDocumentFragment",
                Kind::EntityReference => "DOMEntityReference",
                Kind::Entity => "DOMEntity",
                Kind::Notation => "DOMNotation",
            };
            state
                .node_classes
                .get(class)
                .cloned()
                .map_or(Cow::Borrowed(class), Cow::Owned)
        };
        let value = context.create_object(&class, PhpArray::new())?;
        if let Value::Object(object) = &value {
            self.bind(object);
        }
        Ok(value)
    }

    fn wrap_optional(&self, context: &mut dyn Context, id: Option<NodeId>) -> NativeResult {
        match id {
            Some(id) => self.at(id).wrap(context),
            None => Ok(Value::Null),
        }
    }
}

fn native<T: Clone + 'static>(object: &Object) -> Option<T> {
    object
        .native
        .borrow()
        .as_ref()?
        .downcast_ref::<T>()
        .cloned()
}

fn location(object: &Object) -> Result<Location, NativeError> {
    let handle: NodeHandle = native(object).ok_or_else(|| dom_error(11, "Invalid State Error"))?;
    let location = handle.current();
    if let Ok(mut state) = location.shared.try_borrow_mut() {
        state.release_detached();
    }
    Ok(location)
}

fn receiver(this: This<'_>) -> Result<&Rc<Object>, NativeError> {
    this.ok_or_else(|| NativeError::error("DOM method requires an object"))
}

fn this_node(this: This<'_>) -> Result<Location, NativeError> {
    location(receiver(this)?)
}

fn dom_error(code: i64, message: impl Into<String>) -> NativeError {
    NativeError::Throw {
        class: "DOMException",
        message: message.into(),
        code,
    }
}

fn text(arena: &Arena, id: NodeId) -> String {
    match arena[id].kind {
        Kind::Element | Kind::Fragment | Kind::Document => {
            let mut text = String::new();
            for child in arena.descendants(id) {
                let node = &arena[child];
                match node.kind {
                    Kind::EntityReference => text.push_str(arena.reference_text(child)),
                    Kind::Text | Kind::Cdata
                        if node
                            .parent()
                            .is_none_or(|id| arena[id].kind != Kind::Entity) =>
                    {
                        text.push_str(&node.value);
                    }
                    _ => {}
                }
            }
            text
        }
        Kind::Doctype | Kind::Entity | Kind::Notation => String::new(),
        Kind::EntityReference => arena.reference_text(id).to_owned(),
        _ => arena[id].value.clone(),
    }
}
