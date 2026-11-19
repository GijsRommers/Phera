//! Namespace declarations appear as DOMNameSpaceNode objects in legacy DOM.
//!
//! ```php
//! $node = $root->getAttributeNode('xmlns:p');
//! echo $node->namespaceURI;
//! ```

use phera_extension_api::{
    ClassEntry, ClassHooks, Context, NativeError, NativePropertyAccess, NativePropertyMutation,
    NativePropertyRead, NativeResult, Object, PhpArray, Registry, Value,
};

use crate::tree::{Arena, Kind, NodeId};
use crate::xml::names::XMLNS;

use super::{Location, native};

#[derive(Clone)]
struct Declaration {
    node: Location,
    parent: NodeId,
}

pub(super) fn wrap(context: &mut dyn Context, node: Location) -> NativeResult {
    let parent = {
        let state = node.shared.borrow();
        let arena = &state.document.arena;
        let entry = &arena[node.id];
        if entry.kind == Kind::Attribute && entry.name.namespace.as_deref() == Some(XMLNS) {
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
        *object.native.borrow_mut() = Some(Box::new(Declaration { node, parent }));
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
            match name {
                "nodeName" => Value::from(node.name.qualified()),
                "nodeValue" | "namespaceURI" => Value::from(node.value.as_str()),
                "nodeType" => Value::Int(18),
                "localName" => Value::from(node.name.local.as_str()),
                "prefix" => Value::from(if node.name.prefix.is_some() {
                    node.name.local.as_str()
                } else {
                    ""
                }),
                "isConnected" => {
                    Value::Bool(arena.ancestors(decl.parent).any(|id| id == Arena::DOCUMENT))
                }
                _ => return Ok(None),
            }
        }
    };
    Ok(Some(value))
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
        hooks: Some(&ClassHooks {
            property_access: Some(&NativePropertyAccess { read, mutate }),
            ..ClassHooks::EMPTY
        }),
        ..ClassEntry::class("DOMNameSpaceNode")
    });
}
