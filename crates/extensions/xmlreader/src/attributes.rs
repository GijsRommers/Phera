//! Select an attribute by qualified name, namespace or position.
//!
//! ```php
//! $reader->moveToAttribute('id');
//! echo $reader->value;
//! $reader->moveToElement();
//! ```

use std::rc::Rc;

use phera_ext_dom::tree::{Arena, NodeId};
use phera_extension_api::{Context, NativeError, NativeResult, Object, Value, args};

use crate::cursor::Position;
use crate::state::data;

pub type This<'a> = Option<&'a Rc<Object>>;

#[derive(Clone, Copy)]
enum Match<'a> {
    First,
    Next,
    Index(usize),
    Name(&'a str),
    Namespace { local: &'a str, namespace: &'a str },
}

impl Match<'_> {
    fn matches(self, arena: &Arena, id: NodeId, offset: usize) -> bool {
        let name = &arena[id].name;
        match self {
            Self::First | Self::Next => true,
            Self::Index(wanted) => offset == wanted,
            Self::Name(wanted) => name.qualified() == wanted,
            Self::Namespace { local, namespace } => {
                name.local == local && name.namespace.as_deref().unwrap_or_default() == namespace
            }
        }
    }
}

fn attribute(this: This<'_>, selection: Match<'_>, moving: bool) -> NativeResult {
    let Some(state) = this.and_then(|object| data(object)) else {
        return Ok(if moving {
            Value::Bool(false)
        } else {
            Value::Null
        });
    };
    let mut state = state.borrow_mut();
    let Some(tree) = &state.tree else {
        return Ok(if moving {
            Value::Bool(false)
        } else {
            Value::Null
        });
    };
    let position = match state.position {
        Position::End { id, depth } if !moving => Position::Node { id, depth },
        position => position,
    };
    if !moving && !matches!(position, Position::Node { .. }) {
        return Ok(Value::Null);
    }
    let position = tree.document(|doc| {
        let arena = &doc.arena;
        position.to_attribute(arena, matches!(selection, Match::Next), |id, index| {
            selection.matches(arena, id, index)
        })
    });
    if moving {
        if let Some(position) = position {
            state.position = position;
        } else if matches!(selection, Match::Index(_))
            && let Some(owner) = state.position.to_element()
        {
            state.position = owner;
        }
        Ok(Value::Bool(position.is_some()))
    } else {
        Ok(position.and_then(Position::id).map_or(Value::Null, |id| {
            tree.document(|doc| Value::from(doc.arena[id].value.clone()))
        }))
    }
}

fn name_parameter(method: &str, values: &[Value]) -> Result<String, NativeError> {
    let name = args::bytes(values, 0);
    if name.is_empty() {
        return Err(NativeError::value_error(format!(
            "XMLReader::{method}(): Argument #1 ($name) must not be empty"
        )));
    }
    Ok(String::from_utf8_lossy(name).into_owned())
}

pub fn get(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let name = name_parameter("getAttribute", arguments)?;
    attribute(this, Match::Name(&name), false)
}

pub fn move_to(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let name = name_parameter("moveToAttribute", arguments)?;
    attribute(this, Match::Name(&name), true)
}

pub fn get_number(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let Ok(index) = usize::try_from(args::int(arguments, 0).unwrap_or_default()) else {
        return Ok(Value::Null);
    };
    attribute(this, Match::Index(index), false)
}

pub fn move_to_number(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let Ok(index) = usize::try_from(args::int(arguments, 0).unwrap_or_default()) else {
        return Ok(Value::Bool(false));
    };
    attribute(this, Match::Index(index), true)
}

pub fn get_ns(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let local = name_parameter("getAttributeNs", arguments)?;
    let namespace = namespace_parameter("getAttributeNs", arguments)?;
    attribute(
        this,
        Match::Namespace {
            local: &local,
            namespace: &namespace,
        },
        false,
    )
}

pub fn move_ns(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let local = name_parameter("moveToAttributeNs", arguments)?;
    let namespace = namespace_parameter("moveToAttributeNs", arguments)?;
    attribute(
        this,
        Match::Namespace {
            local: &local,
            namespace: &namespace,
        },
        true,
    )
}

fn namespace_parameter(method: &str, values: &[Value]) -> Result<String, NativeError> {
    let namespace = args::bytes(values, 1);
    if namespace.is_empty() {
        return Err(NativeError::value_error(format!(
            "XMLReader::{method}(): Argument #2 ($namespace) must not be empty"
        )));
    }
    Ok(String::from_utf8_lossy(namespace).into_owned())
}

pub fn first(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    attribute(this, Match::First, true)
}

pub fn next(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    attribute(this, Match::Next, true)
}

pub fn to_element(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let Some(state) = this.and_then(|object| data(object)) else {
        return Ok(Value::Bool(false));
    };
    let mut state = state.borrow_mut();
    if let Some(position) = state.position.to_element() {
        state.position = position;
        return Ok(Value::Bool(true));
    }
    Ok(Value::Bool(false))
}

pub fn lookup_namespace(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let prefix = String::from_utf8_lossy(args::bytes(arguments, 0));
    let Some(state) = this.and_then(|object| data(object)) else {
        return Ok(Value::Null);
    };
    let state = state.borrow();
    let Some(owner) = state.position.owner() else {
        return Ok(Value::Null);
    };
    let Some(tree) = &state.tree else {
        return Ok(Value::Null);
    };
    let namespace = tree.document(|doc| {
        let arena = &doc.arena;
        std::iter::once(owner)
            .chain(arena.ancestors(owner))
            .find_map(|node| {
                arena[node].attributes().iter().find_map(|&attribute| {
                    let node = &arena[attribute];
                    let is_namespace =
                        node.name.namespace.as_deref() == Some("http://www.w3.org/2000/xmlns/");
                    let name = if node.name.prefix.is_some() {
                        &node.name.local
                    } else {
                        ""
                    };
                    (is_namespace && name == prefix).then(|| node.value.clone())
                })
            })
    });
    Ok(namespace.map_or(Value::Null, Value::from))
}
