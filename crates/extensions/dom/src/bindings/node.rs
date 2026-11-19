//! PHP DOMNode navigation, identity, and tree operations.
//!
//! ```php
//! echo $root->firstChild->getNodePath();
//! var_dump($root->isSameNode($document->documentElement));
//! ```

mod compare;
mod registration;

pub(super) use registration::register;

use std::fmt::Write;

use phera_extension_api::{Context, NativeError, NativeResult, Object, Value, args};

use crate::serialize;
use crate::tree::{Arena, Kind, NodeId};
use crate::xml::names::XMLNS;

use super::{This, factory, mutation, this_node};

fn has_children(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let state = node.shared.borrow();
    Ok(Value::Bool(
        state.document.arena[node.id].first_child().is_some(),
    ))
}

fn has_attributes(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let state = node.shared.borrow();
    let arena = &state.document.arena;
    Ok(Value::Bool(arena[node.id].attributes().iter().any(|&id| {
        arena[id].name.namespace.as_deref() != Some(XMLNS)
    })))
}

fn line(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let state = node.shared.borrow();
    let arena = &state.document.arena;
    let mut id = node.id;
    if matches!(arena[id].kind, Kind::Attribute | Kind::Text)
        && arena[id].line == 0
        && let Some(parent) = arena[id].parent()
    {
        id = parent;
    }
    Ok(Value::Int(if arena[id].kind == Kind::Document {
        -1
    } else {
        arena[id].line as i64
    }))
}

fn root(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let id = node
        .shared
        .borrow()
        .document
        .arena
        .ancestors(node.id)
        .last()
        .unwrap_or(node.id);
    node.at(id).wrap(context)
}

fn clone_node(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    mutation::duplicate(
        &this_node(this)?,
        args::bool(arguments, 0).unwrap_or_default(),
    )
    .wrap(context)
}

fn clone_object(source: &std::rc::Rc<Object>, destination: &Object) -> Result<(), NativeError> {
    let node = super::location(source)?;
    let clone = mutation::duplicate(&node, true);
    *destination.native.borrow_mut() = Some(Box::new(clone));
    Ok(())
}

pub(in crate::bindings) fn normalize(
    _: &mut dyn Context,
    this: This<'_>,
    _: &[Value],
) -> NativeResult {
    let node = this_node(this)?;
    let mut state = node.shared.borrow_mut();
    let arena = &mut state.document.arena;
    let mut parents = vec![node.id];
    while let Some(parent) = parents.pop() {
        let mut previous = None;
        let mut cursor = arena[parent].first_child();
        while let Some(id) = cursor {
            cursor = arena[id].next_sibling();
            if arena[id].kind == Kind::Text {
                if arena[id].value.is_empty() {
                    arena.detach(id);
                } else if let Some(previous) = previous {
                    let data = arena[id].value.clone();
                    arena[previous].value.push_str(&data);
                    arena.detach(id);
                } else {
                    previous = Some(id);
                }
            } else {
                previous = None;
                if arena[id].first_child().is_some() {
                    parents.push(id);
                }
            }
        }
    }
    Ok(Value::Null)
}

fn node_path(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let state = node.shared.borrow();
    let arena = &state.document.arena;
    let path = path(arena, node.id);
    Ok(path.map_or(Value::Null, Value::from))
}

fn path(arena: &Arena, root: NodeId) -> Option<String> {
    if matches!(arena[root].kind, Kind::Document | Kind::Doctype) {
        return (arena[root].kind == Kind::Document).then(|| "/".into());
    }
    if arena[root].kind == Kind::Fragment {
        return None;
    }
    let mut segments = Vec::new();
    for id in std::iter::once(root).chain(arena.ancestors(root)) {
        let node = &arena[id];
        if node.kind == Kind::Document {
            break;
        }
        let mut segment = match node.kind {
            Kind::Element if node.name.namespace.is_some() && node.name.prefix.is_none() => {
                "*".into()
            }
            Kind::Text | Kind::Cdata => "text()".into(),
            Kind::Comment => "comment()".into(),
            Kind::ProcessingInstruction => format!("processing-instruction('{}')", node.name.local),
            Kind::Attribute => format!("@{}", node.name.qualified()),
            _ => node.name.qualified(),
        };
        if let Some(parent) = node.parent().filter(|_| node.kind != Kind::Attribute) {
            let mut matches = arena.children(parent).filter(|&candidate| {
                let candidate = &arena[candidate];
                let is_text = |kind| matches!(kind, Kind::Text | Kind::Cdata);
                if is_text(node.kind) {
                    is_text(candidate.kind)
                } else if candidate.kind != node.kind {
                    false
                } else if segment == "*" {
                    candidate.name.prefix.is_none()
                } else {
                    candidate.name == node.name
                }
            });
            let first = matches.next()?;
            if let Some(second) = matches.next() {
                let index = if first == id {
                    1
                } else if second == id {
                    2
                } else {
                    matches
                        .position(|candidate| candidate == id)
                        .map_or(1, |pos| pos + 3)
                };
                let _ = write!(segment, "[{index}]");
            }
        }
        segments.push(segment);
    }
    let mut path = String::new();
    for segment in segments.into_iter().rev() {
        path.push('/');
        path.push_str(&segment);
    }
    Some(path)
}

pub(in crate::bindings) fn namespace_scope(
    arena: &Arena,
    id: NodeId,
) -> impl Iterator<Item = (String, String)> + '_ {
    std::iter::once(id)
        .chain(arena.ancestors(id))
        .flat_map(|id| {
            arena[id].attributes().iter().filter_map(|&id| {
                let attr = &arena[id];
                (attr.name.namespace.as_deref() == Some(XMLNS)).then(|| {
                    let prefix = if attr.name.prefix.is_some() {
                        attr.name.local.clone()
                    } else {
                        String::new()
                    };
                    (prefix, attr.value.clone())
                })
            })
        })
}

fn lookup_namespace(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let prefix = factory::argument(arguments, 0).unwrap_or_default();
    if prefix == "xml" {
        return Ok(Value::from(crate::xml::names::XML));
    }
    let state = node.shared.borrow();
    let uri = namespace_scope(&state.document.arena, node.id)
        .find(|(name, _)| *name == prefix)
        .map(|(_, uri)| uri)
        .filter(|uri| !uri.is_empty());
    Ok(uri.map_or(Value::Null, Value::from))
}

fn lookup_prefix(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let uri = factory::argument(arguments, 0).unwrap_or_default();
    let state = node.shared.borrow();
    let mut seen = std::collections::HashSet::new();
    let prefix = namespace_scope(&state.document.arena, node.id)
        .find(|(prefix, value)| !prefix.is_empty() && seen.insert(prefix.clone()) && *value == uri);
    Ok(prefix.map_or(Value::Null, |(prefix, _)| Value::from(prefix)))
}

fn default_namespace(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let current = lookup_namespace(context, this, &[])?;
    let expected = args::bytes(arguments, 0);
    Ok(Value::Bool(
        matches!(&current, Value::String(value) if value.as_bytes() == expected),
    ))
}

fn c14n(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    if args::array(arguments, 2).is_some() {
        return Err(NativeError::throw(
            "Phera\\NotSupportedException",
            "C14N XPath filters are not supported by Phera",
        ));
    }
    let exclusive = args::bool(arguments, 0).unwrap_or_default();
    if !exclusive && args::array(arguments, 3).is_some() {
        context.notice(
            "DOMNode::C14N(): Inclusive namespace prefixes only allowed in exclusive mode.".into(),
        );
    }
    let state = node.shared.borrow();
    if !state.owned {
        return Ok(Value::Bool(false));
    }
    let document = &state.document;
    let bytes = serialize::canonical(
        document,
        node.id,
        exclusive,
        args::bool(arguments, 1).unwrap_or_default(),
    );
    Ok(Value::from(bytes))
}

fn serialization_forbidden(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let name = super::receiver(this)?.class.name();
    Err(NativeError::throw(
        "Exception",
        format!("Serialization of '{name}' is not allowed"),
    ))
}
