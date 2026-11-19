//! Move nodes in a document and preserve the identity of detached objects.
//!
//! ```php
//! $child = new DOMElement('child');
//! $parent->appendChild($child);
//! assert($child === $parent->firstChild);
//! ```

mod adjacent;
mod ownership;

use ownership::transfer;
pub(super) use ownership::{adopt, adopt_detached, duplicate, import};

pub(super) use adjacent::{
    after, append, before, insert_adjacent_element, insert_adjacent_text, prepend, remove,
    replace_children, replace_with,
};

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::tree::{Arena, Kind, LinkError, NodeId};

use super::{Location, This, dom_error, location, this_node};

pub(super) fn append_child(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    insert(context, this, args, "DOMNode::appendChild", false)
}

pub(super) fn insert_before(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    insert(context, this, args, "DOMNode::insertBefore", false)
}

pub(super) fn replace_child(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    insert(context, this, args, "DOMNode::replaceChild", true)
}

fn insert(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    method: &str,
    replace: bool,
) -> NativeResult {
    let parent = this_node(this)?;
    let child = argument_node(arguments, 0)?;
    let reference = args::object(arguments, 1)
        .map(|object| location(object))
        .transpose()?;
    let reference = match reference {
        Some(reference) if !reference.same_document(&parent) => {
            return fail_at(context, &parent, method, dom_error(8, "Not Found Error"));
        }
        Some(reference) => Some(reference.id),
        None => None,
    };
    if let Some(reference) = reference
        && parent.shared.borrow().document.arena[reference].parent() != Some(parent.id)
    {
        return fail_at(context, &parent, method, dom_error(8, "Not Found Error"));
    }

    let child = match adopt_detached(&parent, child) {
        Ok(child) => child,
        Err(error) => return fail_at(context, &parent, method, error),
    };
    let result = {
        let mut state = parent.shared.borrow_mut();
        if replace && let Some(reference) = reference {
            let arena = &mut state.document.arena;
            let existing = arena[reference].kind == Kind::Attribute;
            let incoming = arena[child.id].kind == Kind::Attribute;
            if existing && incoming {
                arena
                    .set_attribute(parent.id, child.id, Some(reference))
                    .map_err(link_error)?;
                drop(state);
                return parent.at(reference).wrap(context);
            } else if existing || incoming {
                drop(state);
                return fail_at(
                    context,
                    &parent,
                    method,
                    dom_error(3, "Hierarchy Request Error"),
                );
            }
        }
        insert_nodes(&mut state.document.arena, parent.id, child.id, reference)
    };
    let first = match result {
        Ok(Some(id)) => id,
        Ok(None) => {
            if replace && let Some(reference) = reference {
                parent.shared.borrow_mut().remove(reference);
                return parent.at(reference).wrap(context);
            }
            context.warning(format!("{method}(): Document Fragment is empty"));
            return Ok(Value::Bool(false));
        }
        Err(error) => return fail_at(context, &parent, method, error),
    };

    if let Some(reference) = reference.filter(|&id| replace && id != child.id) {
        parent.shared.borrow_mut().remove(reference);
        parent.at(reference).wrap(context)
    } else {
        parent.at(first).wrap(context)
    }
}

pub(super) fn insert_nodes(
    arena: &mut Arena,
    parent: NodeId,
    child: NodeId,
    before: Option<NodeId>,
) -> Result<Option<NodeId>, NativeError> {
    if arena[child].kind == Kind::Fragment {
        if parent == child {
            return Err(dom_error(3, "Hierarchy Request Error"));
        }
        let children: Vec<_> = arena.children(child).collect();
        for &id in &children {
            arena.check_insert(parent, id, before).map_err(link_error)?;
        }
        for &id in &children {
            arena.insert(parent, id, before).map_err(link_error)?;
            super::namespace::reconcile(arena, parent, id);
        }
        Ok(children.first().copied())
    } else if arena[child].kind == Kind::Attribute && arena[parent].kind == Kind::Element {
        use super::attributes::namespaces::{AttributeBinding, bind};
        let mut name = arena[child].name.clone();
        bind(arena, parent, &mut name, AttributeBinding::Attach);
        let existing = arena[parent].attributes().iter().copied().find(|&id| {
            arena[id].name.local == name.local && arena[id].name.namespace == name.namespace
        });
        arena[child].name = name;
        arena
            .set_attribute(parent, child, existing)
            .map_err(link_error)?;
        Ok(Some(child))
    } else {
        arena.insert(parent, child, before).map_err(link_error)?;
        super::namespace::reconcile(arena, parent, child);
        Ok(Some(child))
    }
}

pub(super) fn remove_child(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let parent = this_node(this)?;
    let child = argument_node(args, 0)?;
    if !parent.same_document(&child)
        || parent.shared.borrow().document.arena[child.id].parent() != Some(parent.id)
    {
        return fail_at(
            context,
            &parent,
            "DOMNode::removeChild",
            dom_error(8, "Not Found Error"),
        );
    }
    parent.shared.borrow_mut().remove(child.id);
    child.wrap(context)
}

pub(super) fn argument_node(arguments: &[Value], index: usize) -> Result<Location, NativeError> {
    args::object(arguments, index)
        .ok_or_else(|| NativeError::error("Expected DOMNode"))
        .and_then(|object| location(object))
}

pub(super) fn link_error(error: LinkError) -> NativeError {
    match error {
        LinkError::Hierarchy => dom_error(3, "Hierarchy Request Error"),
        LinkError::MissingReference => dom_error(8, "Not Found Error"),
    }
}

pub(super) fn fail(context: &mut dyn Context, node: &Location, error: NativeError) -> NativeResult {
    let function = context
        .diagnostic_function()
        .unwrap_or("DOMNode")
        .to_owned();
    fail_at(context, node, &function, error)
}

pub(super) fn fail_at(
    context: &mut dyn Context,
    node: &Location,
    function: &str,
    error: NativeError,
) -> NativeResult {
    if !node.shared.borrow().document.settings.strict
        && let NativeError::Throw {
            class: "DOMException",
            message,
            code,
        } = error
    {
        if code == 11 {
            return Err(dom_error(code, message));
        }
        context.warning(format!("{function}(): {message}"));
        return Ok(Value::Bool(false));
    }
    Err(error)
}
