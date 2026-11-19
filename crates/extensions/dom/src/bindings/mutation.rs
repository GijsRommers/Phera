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
    let id = {
        use super::attributes::namespaces::{AttributeBinding, bind};
        let mut destination = target.shared.borrow_mut();
        let arena = &mut destination.document.arena;
        let id = arena.import(
            &source.document.arena,
            node.id,
            args::bool(args, 1).unwrap_or_default(),
        );
        if arena[id].kind == Kind::Attribute {
            let root = arena
                .children(Arena::DOCUMENT)
                .find(|&id| arena[id].kind == Kind::Element);
            if let Some(root) = root {
                let mut name = arena[id].name.clone();
                bind(arena, root, &mut name, AttributeBinding::Node);
                arena[id].name = name;
            }
        }
        id
    };
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
        let clone = Location::new(temporary, true, Arena::DOCUMENT);
        clone
            .shared
            .borrow_mut()
            .node_classes
            .clone_from(&state.node_classes);
        return clone;
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
    if child.shared.borrow().owned {
        return Err(dom_error(4, "Wrong Document Error"));
    }
    Ok(transfer(parent, child))
}

pub(super) fn adopt(
    context: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let target = this_node(this)?;
    let node = argument_node(arguments, 0)?;
    let kind = node.shared.borrow().document.arena[node.id].kind;
    if matches!(
        kind,
        Kind::Document | Kind::Doctype | Kind::Entity | Kind::Notation
    ) {
        return fail_at(
            context,
            &target,
            "DOMDocument::adoptNode",
            dom_error(9, "Not Supported Error"),
        );
    }
    if kind == Kind::Fragment && !node.shared.borrow().owned {
        return Ok(Value::Bool(false));
    }
    if node.same_document(&target) {
        node.shared.borrow_mut().remove(node.id);
        node.wrap(context)
    } else {
        transfer(&target, node).wrap(context)
    }
}

fn transfer(parent: &Location, child: Location) -> Location {
    let mut source = child.shared.borrow_mut();
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
    parent.at(id)
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
            ..
        } = error
    {
        context.warning(format!("{function}(): {message}"));
        return Ok(Value::Bool(false));
    }
    Err(error)
}
