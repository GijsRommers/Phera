//! ParentNode and ChildNode convenience methods.
//!
//! ```php
//! $root->prepend('first');
//! $root->lastChild->after($document->createElement('end'));
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, Value};
use std::collections::HashSet;

use crate::tree::{Kind, Name, Node};

use super::super::{Location, This, dom_error, factory, location, receiver, this_node};
use super::{adopt_detached, argument_node, insert_nodes};

#[derive(Clone, Copy)]
enum Position {
    Append,
    Prepend,
    Before,
    After,
    Replace,
    ReplaceChildren,
}

impl Position {
    fn method(self) -> &'static str {
        match self {
            Self::Append => "append",
            Self::Prepend => "prepend",
            Self::Before => "before",
            Self::After => "after",
            Self::Replace => "replaceWith",
            Self::ReplaceChildren => "replaceChildren",
        }
    }
}

pub(in crate::bindings) fn append(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    change(context, this, args, Position::Append)
}

pub(in crate::bindings) fn prepend(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    change(context, this, args, Position::Prepend)
}

pub(in crate::bindings) fn before(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    change(context, this, args, Position::Before)
}

pub(in crate::bindings) fn after(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    change(context, this, args, Position::After)
}

pub(in crate::bindings) fn replace_with(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    change(context, this, args, Position::Replace)
}

pub(in crate::bindings) fn replace_children(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    change(context, this, args, Position::ReplaceChildren)
}

pub(in crate::bindings) fn remove(
    context: &mut dyn Context,
    this: This<'_>,
    _: &[Value],
) -> NativeResult {
    let node = this_node(this)?;
    let mut state = node.shared.borrow_mut();
    if state.document.arena[node.id].parent().is_none() {
        let error = if state.owned {
            dom_error(8, "Not Found Error")
        } else {
            dom_error(7, "No Modification Allowed Error")
        };
        drop(state);
        let method = format!("{}::remove", receiver(this)?.class.name());
        return super::fail_at(context, &node, &method, error).map(|_| Value::Null);
    }
    state.remove(node.id);
    Ok(Value::Null)
}

fn change(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    position: Position,
) -> NativeResult {
    let node = this_node(this)?;
    let method = format!("{}::{}", receiver(this)?.class.name(), position.method());
    match change_nodes(&node, arguments, position, &method) {
        Ok(_) => Ok(Value::Null),
        Err(error) => super::fail_at(context, &node, &method, error).map(|_| Value::Null),
    }
}

fn change_nodes(
    node: &Location,
    arguments: &[Value],
    position: Position,
    method: &str,
) -> Result<bool, NativeError> {
    let (parent, before) = {
        let state = node.shared.borrow();
        let arena = &state.document.arena;
        let entry = &arena[node.id];
        match position {
            Position::Append | Position::ReplaceChildren => (node.id, None),
            Position::Prepend => (node.id, entry.first_child()),
            Position::Before | Position::Replace | Position::After => {
                let Some(parent) = entry.parent() else {
                    return Ok(false);
                };
                let before = if matches!(position, Position::After) {
                    entry.next_sibling()
                } else {
                    Some(node.id)
                };
                (parent, before)
            }
        }
    };

    let mut nodes = Vec::new();
    for (index, argument) in arguments.iter().enumerate() {
        let argument = argument.deref();
        let id = match &*argument {
            Value::Object(object) if object.class.is_instance_of("DOMNode") => {
                let child = location(object)?;
                if child.shared.borrow().document.arena[child.id].kind == Kind::Attribute {
                    return Err(dom_error(3, "Hierarchy Request Error"));
                }
                match adopt_detached(node, child) {
                    Ok(child) => child.id,
                    Err(error) => {
                        if arguments.len() > 1 {
                            let mut state = node.shared.borrow_mut();
                            for id in nodes {
                                state.remove(id);
                            }
                        }
                        return Err(error);
                    }
                }
            }
            Value::String(value) => node.shared.borrow_mut().document.arena.allocate(Node::new(
                Kind::Text,
                Name::new("#text"),
                String::from_utf8_lossy(value.as_bytes()),
            )),
            _ => {
                return Err(NativeError::type_error(format!(
                    "{method}(): Argument #{} must be of type DOMNode|string, {} given",
                    index + 1,
                    argument.type_name(),
                )));
            }
        };
        nodes.push(id);
    }

    let mut state = node.shared.borrow_mut();
    let arena = &mut state.document.arena;
    let moving: HashSet<_> = nodes.iter().copied().collect();
    let before = std::iter::successors(before, |&id| arena[id].next_sibling())
        .find(|id| !moving.contains(id));
    for &id in &nodes {
        let valid = if arena[id].kind == Kind::Fragment && id != parent {
            arena
                .children(id)
                .try_for_each(|child| arena.check_insert(parent, child, before))
        } else {
            arena.check_insert(parent, id, before)
        };
        if let Err(error) = valid {
            if arguments.len() > 1 {
                for id in nodes {
                    state.remove(id);
                }
            }
            return Err(super::link_error(error));
        }
    }
    if matches!(position, Position::ReplaceChildren) {
        while let Some(id) = arena[parent].first_child() {
            arena.detach(id);
        }
    }
    for &id in &nodes {
        insert_nodes(arena, parent, id, before)?;
    }
    if matches!(position, Position::Replace) && !moving.contains(&node.id) {
        state.remove(node.id);
    }
    Ok(true)
}

pub(in crate::bindings) fn insert_adjacent_element(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let node = this_node(this)?;
    let child = argument_node(args, 1)?;
    let position = adjacent_position(args)?;
    if matches!(position, Position::Before | Position::After)
        && node.shared.borrow().document.arena[node.id]
            .parent()
            .is_none()
    {
        return Ok(Value::Null);
    }
    let child = if node.same_document(&child) {
        child
    } else {
        super::transfer(&node, child)
    };
    let method = "DOMElement::insertAdjacentElement";
    match change_nodes(&node, &args[1..], position, method) {
        Ok(true) => {
            if let Value::Object(object) = &args[1] {
                Ok(Value::Object(object.clone()))
            } else {
                child.wrap(context)
            }
        }
        Ok(false) => Ok(Value::Null),
        Err(error) => super::fail_at(context, &node, method, error),
    }
}

pub(in crate::bindings) fn insert_adjacent_text(
    _: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let node = this_node(this)?;
    let position = adjacent_position(args)?;
    change_nodes(
        &node,
        &args[1..],
        position,
        "DOMElement::insertAdjacentText",
    )?;
    Ok(Value::Null)
}

fn adjacent_position(args: &[Value]) -> Result<Position, NativeError> {
    match factory::argument(args, 0)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "beforebegin" => Ok(Position::Before),
        "afterbegin" => Ok(Position::Prepend),
        "beforeend" => Ok(Position::Append),
        "afterend" => Ok(Position::After),
        _ => Err(dom_error(12, "Syntax Error")),
    }
}
