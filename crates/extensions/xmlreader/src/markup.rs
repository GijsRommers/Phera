//! Read a cursor's text or serialize its current subtree.
//!
//! ```php
//! $reader->read();
//! echo $reader->readInnerXml();
//! $element = $reader->expand();
//! ```

use std::collections::HashSet;

use phera_ext_dom::document::Document;
use phera_ext_dom::serialize::{self, Options};
use phera_ext_dom::shared::SharedNode;
use phera_ext_dom::tree::{Arena, Kind, Name, Node, NodeId};
use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::attributes::This;
use crate::cursor::Position;
use crate::state::data;

const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
const XML: &str = "http://www.w3.org/XML/1998/namespace";

pub fn inner_xml(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(at_node(this, |arena, id, position, doc| {
        let mut output = Vec::new();
        if matches!(position, Position::Node { .. }) {
            for child in arena.children(id) {
                output.extend(copy_xml(doc, child, true));
            }
        }
        Value::from(output)
    }))
}

pub fn outer_xml(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(at_node(this, |_, id, position, doc| {
        let id = position.owner().unwrap_or(id);
        Value::from(copy_xml(doc, id, !matches!(position, Position::End { .. })))
    }))
}

pub fn string(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(at_node(this, |arena, id, position, _| {
        if !matches!(position, Position::Node { .. }) {
            return Value::from("");
        }
        let mut result = String::new();
        for child in std::iter::once(id).chain(arena.descendants(id)) {
            let node = &arena[child];
            if matches!(node.kind, Kind::Text | Kind::Cdata | Kind::EntityReference) {
                result.push_str(&node.value);
            }
        }
        Value::from(result)
    }))
}

fn at_node(
    this: This<'_>,
    use_node: impl FnOnce(&Arena, NodeId, Position, &Document) -> Value,
) -> Value {
    let Some(state) = this.and_then(|object| data(object)) else {
        return Value::from("");
    };
    let state = state.borrow();
    let position = state.position;
    let (Some(tree), Some(id)) = (&state.tree, position.id()) else {
        return Value::from("");
    };
    tree.document(|doc| use_node(&doc.arena, id, position, doc))
}

fn copy_xml(document: &Document, source: NodeId, deep: bool) -> Vec<u8> {
    let tree = &document.arena;
    if tree[source].kind != Kind::Element {
        return serialize::save(document, source, Options::default());
    }
    let mut copy = Document::default();
    let root = copy.arena.import(tree, source, deep);
    let inherited = inherited_bindings(tree, source, deep);
    for (prefix, uri) in inherited {
        let name = if prefix.is_empty() {
            Name {
                local: "xmlns".into(),
                prefix: None,
                namespace: Some(XMLNS.into()),
            }
        } else {
            Name {
                local: prefix,
                prefix: Some("xmlns".into()),
                namespace: Some(XMLNS.into()),
            }
        };
        let attribute = copy.arena.allocate(Node::new(Kind::Attribute, name, uri));
        let _ = copy.arena.set_attribute(root, attribute, None);
    }
    serialize::save(&copy, root, Options::default())
}

fn inherited_bindings(arena: &Arena, root: NodeId, deep: bool) -> Vec<(String, String)> {
    let parents: HashSet<_> = arena.ancestors(root).collect();
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for id in
        std::iter::once(root).chain(
            arena
                .descendants(root)
                .take(if deep { usize::MAX } else { 0 }),
        )
    {
        let node = &arena[id];
        for name in
            std::iter::once(&node.name).chain(node.attributes().iter().map(|&id| &arena[id].name))
        {
            let Some(uri) = &name.namespace else {
                continue;
            };
            if uri == XMLNS || uri == XML {
                continue;
            }
            let prefix = name.prefix.as_deref().unwrap_or_default();
            let provider = std::iter::once(id)
                .chain(arena.ancestors(id))
                .find(|&owner| {
                    arena[owner].attributes().iter().any(|&attribute| {
                        let attr = &arena[attribute];
                        match attr.name.prefix.as_deref() {
                            None if prefix.is_empty() => attr.name.local == "xmlns",
                            Some("xmlns") => attr.name.local == prefix,
                            _ => false,
                        }
                    })
                });
            if provider.is_some_and(|id| parents.contains(&id)) && seen.insert(prefix) {
                result.push((prefix.to_owned(), uri.clone()));
            }
        }
    }
    result
}

pub fn expand(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let state = this
        .and_then(|object| data(object))
        .ok_or_else(|| NativeError::error("Data must be loaded before expanding"))?;
    let state = state.borrow();
    let (Some(tree), Some(node)) = (&state.tree, state.position.owner()) else {
        context.warning("XMLReader::expand(): An Error Occurred while expanding".into());
        return Ok(Value::Bool(false));
    };
    let Some(source) = tree.select(node) else {
        return Ok(Value::Bool(false));
    };
    let destination = args::object(args, 0).and_then(|object| SharedNode::from_dom(object));
    let node = source
        .copy_to(destination.as_ref())
        .ok_or_else(|| NativeError::error("Cannot expand this XML node"))?;
    node.export_dom(context)
}

pub fn unsupported_schema(_: &mut dyn Context, _: This<'_>, _: &[Value]) -> NativeResult {
    Err(NativeError::throw(
        "Phera\\NotSupportedException",
        "XMLReader schema validation is not supported",
    ))
}
