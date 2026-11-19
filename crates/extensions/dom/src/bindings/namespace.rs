//! Namespace declarations appear as DOMNameSpaceNode objects in legacy DOM.
//!
//! ```php
//! $node = $root->getAttributeNode('xmlns:p');
//! echo $node->namespaceURI;
//! ```

mod reconciliation;
mod removal;

pub(super) use reconciliation::reconcile;
pub(super) use removal::remove_by_uri;

use std::rc::Rc;

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassHooks, Context, NativeClone, NativeError, NativePropertyAccess,
    NativePropertyMutation, NativePropertyRead, NativeResult, Object, PhpArray, PropertyPurpose,
    Registry, Value,
};

use crate::tree::{Arena, Kind, Node, NodeId};
use crate::xml::names::{self, XMLNS};

use super::{Location, native};

pub(super) fn declare(arena: &mut Arena, element: NodeId, prefix: &str, uri: &str) {
    let qualified = if prefix.is_empty() {
        "xmlns".into()
    } else {
        format!("xmlns:{prefix}")
    };
    let declaration = arena.allocate(Node::new(
        Kind::Attribute,
        names::qualified(&qualified, Some(XMLNS.into())),
        uri,
    ));
    let _ = arena.set_attribute(element, declaration, None);
}

#[derive(Clone)]
struct Declaration {
    node: Location,
    parent: NodeId,
    _pin: Rc<()>,
}

pub(super) fn location(object: &Object) -> Option<Location> {
    let declaration: Declaration = native(object)?;
    Some(declaration.node)
}

pub(super) fn wrap(context: &mut dyn Context, node: Location) -> NativeResult {
    let parent = {
        let state = node.shared.borrow();
        let arena = &state.document.arena;
        let entry = &arena[node.id];
        if entry.kind == Kind::Namespace
            || (entry.kind == Kind::Attribute && entry.name.namespace.as_deref() == Some(XMLNS))
        {
            entry.parent()
        } else {
            None
        }
    };
    let Some(parent) = parent else {
        return node.wrap(context);
    };
    let result = context.create_object("DOMNameSpaceNode", PhpArray::new())?;
    if let Value::Object(object) = &result {
        let pin = node.shared.borrow_mut().pin(parent);
        *object.native.borrow_mut() = Some(Box::new(Declaration {
            node,
            parent,
            _pin: pin,
        }));
    }
    Ok(result)
}

fn read(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    _: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    let decl: Declaration = native(object)
        .ok_or_else(|| NativeError::error("DOMNameSpaceNode object is uninitialized"))?;
    let value = match name {
        "parentElement" | "parentNode" => decl.node.at(decl.parent).wrap(context)?,
        "ownerDocument" => {
            let owned = decl.node.shared.borrow().owned;
            decl.node
                .wrap_optional(context, owned.then_some(Arena::DOCUMENT))?
        }
        _ => {
            let state = decl.node.shared.borrow();
            let arena = &state.document.arena;
            let node = &arena[decl.node.id];
            let prefix = if node.kind == Kind::Namespace || node.name.prefix.is_some() {
                node.name.local.as_str()
            } else {
                ""
            };
            match name {
                "nodeName" if node.kind == Kind::Namespace && prefix.is_empty() => {
                    Value::from("xmlns")
                }
                "nodeName" if node.kind == Kind::Namespace => {
                    Value::from(format!("xmlns:{prefix}"))
                }
                "nodeName" => Value::from(node.name.qualified()),
                "nodeValue" | "namespaceURI" if node.value.is_empty() => Value::Null,
                "nodeValue" | "namespaceURI" => Value::from(node.value.as_str()),
                "nodeType" => Value::Int(18),
                "localName" if node.kind == Kind::Namespace && prefix.is_empty() => {
                    Value::from("xmlns")
                }
                "localName" => Value::from(node.name.local.as_str()),
                "prefix" => Value::from(prefix),
                "isConnected" => {
                    Value::Bool(arena.ancestors(decl.parent).any(|id| id == Arena::DOCUMENT))
                }
                _ => return Ok(None),
            }
        }
    };
    Ok(Some(value))
}

fn debug(
    context: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let mut table = object.to_array();
    if purpose == PropertyPurpose::Debug && native::<Declaration>(object).is_some() {
        for name in [
            "nodeName",
            "nodeValue",
            "nodeType",
            "prefix",
            "localName",
            "namespaceURI",
            "isConnected",
            "ownerDocument",
            "parentNode",
            "parentElement",
        ] {
            let value = if matches!(name, "ownerDocument" | "parentNode" | "parentElement") {
                Value::from("(object value omitted)")
            } else {
                read(context, object, name, NativePropertyRead::Read)?.unwrap_or_default()
            };
            table.insert(ArrayKey::str(name), value);
        }
    }
    Ok(table)
}

fn mutate(
    _: &mut dyn Context,
    _: &Object,
    name: &str,
    _: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if matches!(
        name,
        "nodeName"
            | "nodeValue"
            | "namespaceURI"
            | "nodeType"
            | "localName"
            | "prefix"
            | "isConnected"
            | "parentElement"
            | "parentNode"
            | "ownerDocument"
    ) {
        return Err(NativeError::error(format!(
            "Cannot modify readonly property DOMNameSpaceNode::${name}"
        )));
    }
    Ok(false)
}

pub(super) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        hooks: const {
            Some(&ClassHooks {
                clone: Some(NativeClone::copy::<Declaration>()),
                properties: Some(debug),
                property_access: Some(&NativePropertyAccess { read, mutate }),
                ..ClassHooks::EMPTY
            })
        },
        ..ClassEntry::class("DOMNameSpaceNode")
    });
}
