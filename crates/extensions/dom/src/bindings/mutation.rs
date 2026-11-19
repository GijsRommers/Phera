//! Move nodes in a document and preserve the identity of detached objects.
//!
//! ```php
//! $child = new DOMElement('child');
//! $parent->appendChild($child);
//! assert($child === $parent->firstChild);
//! ```

mod adjacent;

pub(super) use adjacent::{
    after, append, before, insert_adjacent_element, insert_adjacent_text, prepend, remove,
    replace_children, replace_with,
};

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::document::Document;
use crate::tree::{Arena, Kind, LinkError, NodeId};

use super::{Location, This, dom_error, location, this_node};

pub(super) fn append_child(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    insert(context, this, args, false)
}

pub(super) fn insert_before(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    insert(context, this, args, false)
}

pub(super) fn replace_child(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    insert(context, this, args, true)
}

fn insert(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
    replace: bool,
) -> NativeResult {
    let parent = this_node(this)?;
    let child = argument_node(arguments, 0)?;
    let reference = args::object(arguments, 1)
        .map(|object| location(object))
        .transpose()?;
    let reference = match reference {
        Some(reference) if !reference.same_document(&parent) => {
            return fail(context, &parent, dom_error(8, "Not Found Error"));
        }
        Some(reference) => Some(reference.id),
        None => None,
    };
    if let Some(reference) = reference
        && parent.shared.borrow().document.arena[reference].parent() != Some(parent.id)
    {
        return fail(context, &parent, dom_error(8, "Not Found Error"));
    }

    let child = match adopt_detached(&parent, child) {
        Ok(child) => child,
        Err(error) => return fail(context, &parent, error),
    };
    let result = {
        let mut state = parent.shared.borrow_mut();
        insert_nodes(&mut state.document.arena, parent.id, child.id, reference)
    };
    let first = match result {
        Ok(Some(id)) => id,
        Ok(None) => {
            let function = context
                .diagnostic_function()
                .unwrap_or("DOMNode::appendChild");
            context.warning(format!("{function}(): Document Fragment is empty"));
            return Ok(Value::Bool(false));
        }
        Err(error) => return fail(context, &parent, error),
    };

    if let Some(reference) = reference.filter(|&id| replace && id != child.id) {
        parent.shared.borrow_mut().document.arena.detach(reference);
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
        }
        Ok(children.first().copied())
    } else if arena[child].kind == Kind::Attribute && arena[parent].kind == Kind::Element {
        let existing = arena[parent].attributes().iter().copied().find(|&id| {
            arena[id].name.local == arena[child].name.local
                && arena[id].name.namespace == arena[child].name.namespace
        });
        arena
            .set_attribute(parent, child, existing)
            .map_err(link_error)?;
        Ok(Some(child))
    } else {
        arena.insert(parent, child, before).map_err(link_error)?;
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
        return fail(context, &parent, dom_error(8, "Not Found Error"));
    }
    parent.shared.borrow_mut().document.arena.detach(child.id);
    child.wrap(context)
}

pub(super) fn import(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let target = this_node(this)?;
    let node = argument_node(args, 0)?;
    if target.same_document(&node) {
        return node.wrap(context);
    }
    let source = node.shared.borrow();
    if matches!(
        source.document.arena[node.id].kind,
        Kind::Document | Kind::Doctype
    ) {
        context.warning("DOMDocument::importNode(): Cannot import: Node Type Not Supported".into());
        return Ok(Value::Bool(false));
    }
    let id = target.shared.borrow_mut().document.arena.import(
        &source.document.arena,
        node.id,
        args::bool(args, 1).unwrap_or_default(),
    );
    drop(source);
    target.at(id).wrap(context)
}

pub(super) fn duplicate(node: &Location, deep: bool) -> Location {
    let mut temporary = Document::default();
    let state = node.shared.borrow();
    let source = &state.document.arena;
    let is_document = source[node.id].kind == Kind::Document;
    if is_document {
        temporary.settings = state.document.settings.clone();
        if deep {
            for child in source.children(node.id) {
                let copy = temporary.arena.import(source, child, true);
                let _ = temporary.arena.insert(Arena::DOCUMENT, copy, None);
            }
        }
        return Location::new(temporary, true, Arena::DOCUMENT);
    }
    let copy = temporary.arena.import(source, node.id, deep);
    if !state.owned {
        return Location::new(temporary, false, copy);
    }
    drop(state);
    let copy = node
        .shared
        .borrow_mut()
        .document
        .arena
        .import(&temporary.arena, copy, deep);
    node.at(copy)
}

pub(super) fn adopt_detached(parent: &Location, child: Location) -> Result<Location, NativeError> {
    if parent.same_document(&child) {
        return Ok(child);
    }
    let mut source = child.shared.borrow_mut();
    if source.owned {
        return Err(dom_error(4, "Wrong Document Error"));
    }
    let (id, mapping) = parent.shared.borrow_mut().document.arena.import_mapping(
        &source.document.arena,
        child.id,
        true,
    );
    for (previous, new) in mapping {
        if let Some(object) = source
            .wrappers
            .remove(&previous)
            .and_then(|object| object.upgrade())
        {
            parent.at(new).bind(&object);
        }
    }
    source.document.arena.detach(child.id);
    Ok(parent.at(id))
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
    if !node.shared.borrow().document.settings.strict
        && let NativeError::Throw {
            class: "DOMException",
            message,
            ..
        } = error
    {
        let function = context.diagnostic_function().unwrap_or("DOMNode");
        context.warning(format!("{function}(): {message}"));
        return Ok(Value::Bool(false));
    }
    Err(error)
}
