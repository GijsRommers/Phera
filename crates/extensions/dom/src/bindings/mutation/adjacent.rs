//! ParentNode and ChildNode convenience methods.
//!
//! ```php
//! $root->prepend('first');
//! $root->lastChild->after($document->createElement('end'));
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, Value};

use crate::tree::{Kind, Name, Node};

use super::super::{Location, This, factory, location, this_node};
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
    _: &mut dyn Context,
    this: This<'_>,
    _: &[Value],
) -> NativeResult {
    let node = this_node(this)?;
    node.shared.borrow_mut().document.arena.detach(node.id);
    Ok(Value::Null)
}

fn change(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    position: Position,
) -> NativeResult {
    let node = this_node(this)?;
    match change_nodes(&node, arguments, position) {
        Ok(_) => Ok(Value::Null),
        Err(error) => super::fail(context, &node, error).map(|_| Value::Null),
    }
}

fn change_nodes(
    node: &Location,
    arguments: &[Value],
    position: Position,
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
    for argument in arguments {
        let argument = argument.deref();
        let id = match &*argument {
            Value::Object(object) if object.class.is_instance_of("DOMNode") => {
                adopt_detached(node, location(object)?)?.id
            }
            Value::String(value) => node.shared.borrow_mut().document.arena.allocate(Node::new(
                Kind::Text,
                Name::new("#text"),
                String::from_utf8_lossy(value.as_bytes()),
            )),
            _ => {
                return Err(NativeError::type_error(
                    "Argument must be of type DOMNode|string",
                ));
            }
        };
        nodes.push(id);
    }

    let mut state = node.shared.borrow_mut();
    let arena = &mut state.document.arena;
    for &id in &nodes {
        if arena[id].kind == Kind::Fragment {
            for child in arena.children(id) {
                arena
                    .check_insert(parent, child, before)
                    .map_err(super::link_error)?;
            }
        } else if arena[id].kind != Kind::Attribute {
            arena
                .check_insert(parent, id, before)
                .map_err(super::link_error)?;
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
    if matches!(position, Position::Replace) && !nodes.contains(&node.id) {
        arena.detach(node.id);
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
    let position = adjacent_position(args, "insertAdjacentElement")?;
    match change_nodes(&node, &args[1..], position) {
        Ok(true) => {
            if let Value::Object(object) = &args[1] {
                Ok(Value::Object(object.clone()))
            } else {
                child.wrap(context)
            }
        }
        Ok(false) => Ok(Value::Null),
        Err(error) => super::fail(context, &node, error),
    }
}

pub(in crate::bindings) fn insert_adjacent_text(
    _: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let node = this_node(this)?;
    let position = adjacent_position(args, "insertAdjacentText")?;
    change_nodes(&node, &args[1..], position)?;
    Ok(Value::Null)
}

fn adjacent_position(args: &[Value], method: &str) -> Result<Position, NativeError> {
    match factory::argument(args, 0)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "beforebegin" => Ok(Position::Before),
        "afterbegin" => Ok(Position::Prepend),
        "beforeend" => Ok(Position::Append),
        "afterend" => Ok(Position::After),
        _ => Err(NativeError::value_error(format!(
            "DOMElement::{method}(): Argument #1 ($where) must be one of \"beforebegin\", \
             \"afterbegin\", \"beforeend\", or \"afterend\""
        ))),
    }
}
