//! Copy or transfer a subtree between DOM arenas and update live wrappers.
//!
//! ```php
//! $target->appendChild($target->adoptNode($source->documentElement));
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::document::Document;
use crate::tree::{Arena, Kind};

use super::super::{Location, This, dom_error, this_node};
use super::{argument_node, fail_at};

pub(in crate::bindings) fn import(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
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
        use super::super::attributes::namespaces::{AttributeBinding, bind};
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

pub(in crate::bindings) fn duplicate(node: &Location, deep: bool) -> Location {
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

pub(in crate::bindings) fn adopt_detached(
    parent: &Location,
    child: Location,
) -> Result<Location, NativeError> {
    if parent.same_document(&child) {
        return Ok(child);
    }
    if child.shared.borrow().owned {
        return Err(dom_error(4, "Wrong Document Error"));
    }
    Ok(transfer(parent, child))
}

pub(in crate::bindings) fn adopt(
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

pub(super) fn transfer(parent: &Location, child: Location) -> Location {
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
