//! Read and replace element markup with the appropriate fragment parser.
//!
//! ```php
//! $element->innerHTML = '<strong>updated</strong>';
//! echo $element->innerHTML;
//! ```

mod registration;

pub(super) use registration::register;

use std::collections::HashSet;
use std::fmt::Write;

use phera_extension_api::{NativeError, Value};

use crate::document::Document;
use crate::tree::{Arena, NodeId};
use crate::{html, serialize, xml};

use super::super::{Location, dom_error, namespace, node};

pub(super) fn inner_html(location: &Location) -> Value {
    let state = location.shared.borrow();
    let document = &state.document;
    let html = document.settings.html;
    let options = serialize::Options {
        html,
        ..serialize::Options::default()
    };
    let mut output = Vec::new();
    for child in document.arena.children(location.id) {
        output.extend(if html {
            serialize::save(document, child, options)
        } else {
            serialize_xml_fragment(document, child, options)
        });
    }
    Value::from(output)
}

pub(super) fn outer_html(location: &Location) -> Value {
    let state = location.shared.borrow();
    let document = &state.document;
    let options = serialize::Options {
        html: document.settings.html,
        ..serialize::Options::default()
    };
    Value::from(if options.html {
        serialize::save(document, location.id, options)
    } else {
        serialize_xml_fragment(document, location.id, options)
    })
}

fn serialize_xml_fragment(document: &Document, id: NodeId, options: serialize::Options) -> Vec<u8> {
    let mut isolated = Document {
        settings: document.settings.clone(),
        ..Document::default()
    };
    let root = isolated.arena.import(&document.arena, id, true);
    namespace::declare_import_bindings(&mut isolated.arena, root);
    serialize::save(&isolated, root, options)
}

pub(super) fn set_inner_html(location: &Location, input: &str) -> Result<(), NativeError> {
    let (source, root) = {
        let state = location.shared.borrow();
        let document = &state.document;
        if document.settings.html {
            html::fragment(input, &document.arena[location.id].name)
        } else {
            xml_fragment(&document.arena, location.id, input)?
        }
    };
    let mut state = location.shared.borrow_mut();
    while let Some(id) = state.document.arena[location.id].first_child() {
        state.remove(id);
    }
    let arena = &mut state.document.arena;
    for child in source.children(root) {
        let id = arena.import(&source, child, true);
        arena
            .insert(location.id, id, None)
            .map_err(super::super::mutation::link_error)?;
    }
    Ok(())
}

fn xml_fragment(
    arena: &Arena,
    element: NodeId,
    input: &str,
) -> Result<(Arena, NodeId), NativeError> {
    let mut source = String::from("<fragment");
    let mut seen = HashSet::new();
    for (prefix, uri) in node::namespace_scope(arena, element) {
        if seen.insert(prefix.clone()) {
            let prefix = if prefix.is_empty() {
                prefix
            } else {
                format!(":{prefix}")
            };
            let uri = uri
                .replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;");
            let _ = write!(source, " xmlns{prefix}=\"{uri}\"");
        }
    }
    let _ = write!(source, ">{input}</fragment>");
    let parsed = xml::parse(source.as_bytes(), xml::Options::default());
    let document = parsed
        .document
        .filter(|_| parsed.diagnostics.iter().all(|error| error.level < 2))
        .ok_or_else(|| dom_error(12, "XML fragment is not well-formed"))?;
    let root = document.arena[Arena::DOCUMENT]
        .first_child()
        .ok_or_else(|| dom_error(12, "XML fragment is not well-formed"))?;
    Ok((document.arena, root))
}
