//! Legacy DOMElement and DOMAttr attribute and ID operations.
//!
//! ```php
//! $root->setAttribute('id', 'target');
//! $root->setIdAttribute('id', true);
//! assert($document->getElementById('target') === $root);
//! ```

pub(super) mod namespaces;
mod registration;

pub(super) use registration::register;

use phera_extension_api::{Context, NativeError, NativeResult, PhpArray, Value, args};

use crate::tree::{Arena, Kind, Name, Node, NodeId};
use crate::xml::names::{self, XML, XMLNS};

use super::{This, dom_error, factory, mutation, this_node};

pub(super) fn find(
    arena: &Arena,
    id: NodeId,
    namespace: Option<Option<&str>>,
    name: &str,
) -> Option<NodeId> {
    arena[id].attributes().iter().copied().find(|&id| {
        let attr = &arena[id].name;
        match namespace {
            Some(namespace) => attr.namespace.as_deref() == namespace && attr.local == name,
            None => attr.qualified() == name,
        }
    })
}

pub(super) fn set(
    arena: &mut Arena,
    id: NodeId,
    name: Name,
    value: String,
    html: bool,
) -> Result<NodeId, NativeError> {
    let mut name = name;
    namespaces::bind(arena, id, &mut name, namespaces::AttributeBinding::Direct);
    let existing = if name.namespace.is_none() {
        find(arena, id, None, &name.qualified())
    } else {
        find(arena, id, Some(name.namespace.as_deref()), &name.local)
    };
    if let Some(existing) = existing {
        arena[existing].name = name;
        arena.assign_attribute(existing, value);
        return Ok(existing);
    }
    let mut attribute = Node::new(Kind::Attribute, name, value);
    attribute.is_id = attribute.name.local == "id"
        && (attribute.name.namespace.as_deref() == Some(XML)
            || (html && attribute.name.namespace.is_none()));
    let attribute = arena.attribute(attribute, true);
    arena
        .set_attribute(id, attribute, None)
        .map_err(mutation::link_error)?;
    Ok(attribute)
}

#[derive(Clone, Copy)]
enum Read {
    Value,
    Has,
    Node,
    Remove,
    MarkId,
}

fn read(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    ns: bool,
    operation: Read,
) -> NativeResult {
    let node = this_node(this)?;
    let namespace = ns.then(|| factory::namespace(arguments, 0));
    let name = factory::argument(arguments, usize::from(ns)).unwrap_or_default();
    let attribute = {
        let state = node.shared.borrow();
        let namespace = namespace.as_ref().map(|uri| uri.as_deref());
        find(&state.document.arena, node.id, namespace, &name)
    };
    match (operation, attribute) {
        (Read::Value, Some(attribute)) => {
            let state = node.shared.borrow();
            Ok(Value::from(state.document.arena[attribute].value.as_str()))
        }
        (Read::Value, None) => Ok(Value::from("")),
        (Read::Has, attribute) => Ok(Value::Bool(attribute.is_some())),
        (Read::Node, Some(attribute)) => super::namespace::wrap(context, node.at(attribute)),
        (Read::Node, None) => Ok(if ns { Value::Null } else { Value::Bool(false) }),
        (Read::Remove, Some(attribute)) => {
            node.shared.borrow_mut().remove(attribute);
            Ok(if ns { Value::Null } else { Value::Bool(true) })
        }
        (Read::Remove, None) => Ok(if ns { Value::Null } else { Value::Bool(false) }),
        (Read::MarkId, Some(attribute)) => {
            node.shared.borrow_mut().document.arena[attribute].is_id =
                args::bool(arguments, if ns { 2 } else { 1 }).unwrap_or_default();
            Ok(Value::Null)
        }
        (Read::MarkId, None) => {
            mutation::fail(context, &node, dom_error(8, "Not Found Error")).map(|_| Value::Null)
        }
    }
}

fn set_attr(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    ns: bool,
) -> NativeResult {
    let node = this_node(this)?;
    let namespace = if ns {
        factory::namespace(arguments, 0)
    } else {
        None
    };
    let name = factory::argument(arguments, usize::from(ns)).unwrap_or_default();
    let value = factory::argument(arguments, usize::from(ns) + 1).unwrap_or_default();
    let mut name = match factory::checked_name(name, namespace, ns) {
        Ok(name) => name,
        Err(error) => return mutation::fail(context, &node, error),
    };
    let id = {
        let mut state = node.shared.borrow_mut();
        let document = &mut state.document;
        if !ns && let Some((prefix, _)) = name.local.split_once(':') {
            let uri = if prefix == "xml" {
                Some(XML.to_owned())
            } else {
                super::node::namespace_scope(&document.arena, node.id)
                    .find(|(candidate, _)| candidate == prefix)
                    .map(|(_, uri)| uri)
            };
            if let Some(uri) = uri {
                name = names::qualified(&name.local, Some(uri));
            }
        }
        set(
            &mut document.arena,
            node.id,
            name,
            value,
            document.settings.html,
        )?
    };
    if ns {
        Ok(Value::Null)
    } else {
        node.at(id).wrap(context)
    }
}

fn toggle(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let name = factory::argument(arguments, 0).unwrap_or_default();
    let name = match factory::checked_name(name, None, false) {
        Ok(name) => name,
        Err(error) => return mutation::fail(context, &node, error),
    };
    let mut state = node.shared.borrow_mut();
    let html = state.document.settings.html;
    let arena = &mut state.document.arena;
    let previous = find(arena, node.id, None, &name.local);
    let enabled = args::bool(arguments, 1).unwrap_or(previous.is_none());
    match (enabled, previous) {
        (false, Some(id)) => arena.detach(id),
        (true, None) => {
            set(arena, node.id, name, String::new(), html)?;
        }
        _ => {}
    }
    Ok(Value::Bool(enabled))
}

fn names(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let state = node.shared.borrow();
    let arena = &state.document.arena;
    let mut values = PhpArray::new();
    for &attribute in arena[node.id].attributes() {
        let attr = &arena[attribute];
        if attr.name.namespace.as_deref() != Some(XMLNS) {
            let _ = values.push(Value::from(attr.name.qualified()));
        }
    }
    Ok(Value::from(values))
}

fn is_id(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    Ok(Value::Bool(
        node.shared.borrow().document.arena[node.id].is_id,
    ))
}

fn set_attribute_node(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    by_namespace: bool,
) -> NativeResult {
    let element = this_node(this)?;
    let attr = mutation::argument_node(arguments, 0)?;
    let attr = match mutation::adopt_detached(&element, attr) {
        Ok(attr) => attr,
        Err(error) => return mutation::fail(context, &element, error),
    };
    let result = {
        let mut state = element.shared.borrow_mut();
        let arena = &mut state.document.arena;
        if arena[attr.id]
            .parent()
            .is_some_and(|parent| parent != element.id)
        {
            Err(dom_error(10, "Inuse Attribute Error"))
        } else {
            let mut name = arena[attr.id].name.clone();
            let old = if by_namespace {
                find(
                    arena,
                    element.id,
                    Some(name.namespace.as_deref()),
                    &name.local,
                )
            } else {
                arena[element.id].attributes().iter().copied().find(|&id| {
                    arena[id].name.local == name.local
                        && arena[id].name.namespace.as_deref() != Some(XMLNS)
                })
            };
            namespaces::bind(
                arena,
                element.id,
                &mut name,
                namespaces::AttributeBinding::Node,
            );
            arena[attr.id].name = name;
            arena
                .set_attribute(element.id, attr.id, old)
                .map_err(mutation::link_error)?;
            Ok(old.filter(|&id| id != attr.id))
        }
    };
    match result {
        Ok(value) => element.wrap_optional(context, value),
        Err(error) => mutation::fail(context, &element, error),
    }
}

fn mark_node(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let element = this_node(this)?;
    let attr = mutation::argument_node(arguments, 0)?;
    if !element.same_document(&attr)
        || element.shared.borrow().document.arena[attr.id].parent() != Some(element.id)
    {
        return mutation::fail(context, &element, dom_error(8, "Not Found Error"))
            .map(|_| Value::Null);
    }
    element.shared.borrow_mut().document.arena[attr.id].is_id =
        args::bool(arguments, 1).unwrap_or_default();
    Ok(Value::Null)
}
