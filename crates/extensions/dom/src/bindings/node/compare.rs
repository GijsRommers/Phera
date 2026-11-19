//! Compare node identity, content, and document position.
//!
//! ```php
//! var_dump($root->isEqualNode($root->cloneNode(true)));
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use super::super::{This, location, mutation::argument_node, this_node};

pub(super) fn same(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let other = argument_node(arguments, 0)?;
    Ok(Value::Bool(
        node.same_document(&other) && node.id == other.id,
    ))
}

pub(super) fn contains(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let node = this_node(this)?;
    let other = args::value(arguments, 0);
    let other = match &*other.deref() {
        Value::Null => return Ok(Value::Bool(false)),
        Value::Object(object) if object.class.is_instance_of("DOMNode") => location(object)?,
        Value::Object(object) if object.class.is_instance_of("DOMNameSpaceNode") => {
            super::super::namespace::location(object)
                .ok_or_else(|| super::super::dom_error(11, "Invalid State Error"))?
        }
        other => {
            return Err(NativeError::type_error(format!(
                "DOMNode::contains(): Argument #1 ($other) must be of type \
             DOMNode|DOMNameSpaceNode|null, {} given",
                other.type_name(),
            )));
        }
    };
    let contains = node.same_document(&other)
        && (node.id == other.id || {
            let state = node.shared.borrow();
            state
                .document
                .arena
                .ancestors(other.id)
                .any(|id| id == node.id)
        });
    Ok(Value::Bool(contains))
}

pub(super) fn equal(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let left = this_node(this)?;
    let Some(other) = args::object(arguments, 0) else {
        return Ok(Value::Bool(false));
    };
    let right = location(other)?;
    let left_state = left.shared.borrow();
    let right_state = right.shared.borrow();
    let left_arena = &left_state.document.arena;
    let right_arena = &right_state.document.arena;
    let mut pending = vec![(left.id, right.id)];
    while let Some((left, right)) = pending.pop() {
        let a = &left_arena[left];
        let b = &right_arena[right];
        if a.kind != b.kind
            || a.name != b.name
            || a.value != b.value
            || a.doctype != b.doctype
            || a.attributes().len() != b.attributes().len()
        {
            return Ok(Value::Bool(false));
        }
        for &attribute in a.attributes() {
            let lhs = &left_arena[attribute];
            if !b.attributes().iter().any(|&attribute| {
                let rhs = &right_arena[attribute];
                lhs.name == rhs.name && lhs.value == rhs.value
            }) {
                return Ok(Value::Bool(false));
            }
        }
        let mut right = right_arena.children(right);
        for child in left_arena.children(left) {
            let Some(peer) = right.next() else {
                return Ok(Value::Bool(false));
            };
            pending.push((child, peer));
        }
        if right.next().is_some() {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

pub(super) fn position(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let left = this_node(this)?;
    let right = argument_node(arguments, 0)?;
    if !left.same_document(&right) {
        let left_key = std::rc::Rc::as_ptr(&left.shared);
        let right_key = std::rc::Rc::as_ptr(&right.shared);
        return Ok(Value::Int(
            1 | 32 | if left_key < right_key { 4 } else { 2 },
        ));
    }
    if left.id == right.id {
        return Ok(Value::Int(0));
    }

    let state = left.shared.borrow();
    let arena = &state.document.arena;
    let mut left_path: Vec<_> = std::iter::once(left.id)
        .chain(arena.ancestors(left.id))
        .collect();
    let mut right_path: Vec<_> = std::iter::once(right.id)
        .chain(arena.ancestors(right.id))
        .collect();
    if left_path.last() != right_path.last() {
        return Ok(Value::Int(
            1 | 32 | if left.id.0 < right.id.0 { 4 } else { 2 },
        ));
    }
    let mut common = None;
    while left_path.last().is_some() && left_path.last() == right_path.last() {
        common = left_path.pop();
        right_path.pop();
    }
    let value = match (left_path.last(), right_path.last(), common) {
        (None, Some(_), _) => 4 | 16,
        (Some(_), None, _) => 2 | 8,
        (Some(&a), Some(&b), Some(parent)) => {
            let order = arena[parent]
                .attributes()
                .iter()
                .copied()
                .chain(arena.children(parent))
                .find(|&candidate| candidate == a || candidate == b);
            if order == Some(a) { 4 } else { 2 }
        }
        _ => 0,
    };
    Ok(Value::Int(value))
}
