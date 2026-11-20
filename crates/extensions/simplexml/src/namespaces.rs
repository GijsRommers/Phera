//! Read namespace declarations from the same arena used by DOM XPath.
//!
//! ```php
//! $xml->registerXPathNamespace('p', 'urn:example');
//! $elements = $xml->xpath('//p:item');
//! ```

use std::collections::HashMap;
use std::rc::Rc;

use phera_ext_dom::tree::{Arena, Kind, Name, NodeId};
use phera_ext_dom::xpath;
use phera_extension_api::{Context, NativeError, NativeResult, Object, PhpArray, Value, args};

use crate::selection::{Filter, Selected, Selection, data, wrap};

type This<'a> = Option<&'a Rc<Object>>;
const XML: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS: &str = "http://www.w3.org/2000/xmlns/";

fn receiver(this: This<'_>) -> Result<&Object, NativeError> {
    this.map(Rc::as_ref)
        .ok_or_else(|| NativeError::error("SimpleXMLElement is not properly initialized"))
}

fn declarations(arena: &Arena, id: NodeId) -> impl Iterator<Item = (&str, &str)> {
    arena[id].attributes().iter().filter_map(|&id| {
        let node = &arena[id];
        if node.name.namespace.as_deref() == Some(XMLNS) {
            Some((
                if node.name.prefix.is_none() {
                    ""
                } else {
                    node.name.local.as_str()
                },
                node.value.as_str(),
            ))
        } else {
            None
        }
    })
}

pub fn lookup(arena: &Arena, node: NodeId, prefix: &str) -> Option<String> {
    if prefix == "xml" {
        return Some(XML.to_owned());
    }
    std::iter::once(node)
        .chain(arena.ancestors(node))
        .find_map(|node| {
            declarations(arena, node)
                .find(|(candidate, _)| *candidate == prefix)
                .map(|(_, uri)| uri.to_owned())
        })
}

pub fn prefix_for_uri(arena: &Arena, node: NodeId, uri: &str, attribute: bool) -> Option<String> {
    if uri == XML {
        return Some("xml".to_owned());
    }
    let mut shadowed = std::collections::HashSet::new();
    for owner in std::iter::once(node).chain(arena.ancestors(node)) {
        for (prefix, value) in declarations(arena, owner) {
            if shadowed.insert(prefix) && value == uri && (!attribute || !prefix.is_empty()) {
                return Some(prefix.to_owned());
            }
        }
    }
    None
}

pub fn register_xpath(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    let prefix = String::from_utf8_lossy(args::bytes(arguments, 0));
    let uri = String::from_utf8_lossy(args::bytes(arguments, 1));
    if prefix.is_empty() {
        return Ok(Value::Bool(false));
    }
    let mut namespaces = state.namespaces.borrow_mut();
    if uri.is_empty() {
        Ok(Value::Bool(namespaces.remove(prefix.as_ref()).is_some()))
    } else {
        namespaces.insert(prefix.into_owned(), uri.into_owned());
        Ok(Value::Bool(true))
    }
}

pub fn query(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let state = data(object)?;
    if matches!(state.selection.kind, Selected::Attributes(_)) {
        return Ok(Value::Null);
    }
    let Some(node) = state.selection.first() else {
        return Ok(Value::Null);
    };
    let expression = String::from_utf8_lossy(args::bytes(arguments, 0));
    let result = node.document_mut(|doc| {
        let arena = &mut doc.arena;
        let mut namespaces = HashMap::from([("xml".to_owned(), XML.to_owned())]);
        for owner in std::iter::once(node.id()).chain(arena.ancestors(node.id())) {
            for (prefix, uri) in declarations(arena, owner) {
                namespaces
                    .entry(prefix.to_owned())
                    .or_insert_with(|| uri.to_owned());
            }
        }
        namespaces.extend(state.namespaces.borrow().clone());
        xpath::evaluate(arena, node.id(), &namespaces, &expression)
    });
    let items = match result {
        Ok(xpath::Value::Nodes(items)) => items,
        Ok(value) => {
            let returned = match value {
                xpath::Value::Boolean(_) => "bool",
                xpath::Value::String(_) => "string",
                xpath::Value::Number(_) => "number",
                xpath::Value::Nodes(_) => return Ok(Value::Bool(false)),
            };
            context.warning(format!(
                "SimpleXMLElement::xpath(): XPath expression must return a node set, {returned} returned"
            ));
            return Ok(Value::Bool(false));
        }
        Err(error) => {
            context.warning(format!("SimpleXMLElement::xpath(): {}", error.message()));
            return Ok(Value::Bool(false));
        }
    };
    let mut result = PhpArray::new();
    for item in items {
        let selection = node.document(|doc| {
            let entry = &doc.arena[item];
            match entry.kind {
                Kind::Text => Some((entry.parent()?, Selected::Node, Filter::Default)),
                Kind::Attribute if entry.name.namespace.as_deref() != Some(XMLNS) => Some((
                    entry.parent()?,
                    Selected::Attributes(Some(entry.name.local.clone())),
                    Filter::new(entry.name.namespace.as_deref(), false),
                )),
                Kind::Element | Kind::Comment | Kind::ProcessingInstruction => {
                    Some((item, Selected::Node, Filter::Default))
                }
                _ => None,
            }
        });
        let Some((id, kind, filter)) = selection else {
            continue;
        };
        let Some(anchor) = node.select(id) else {
            continue;
        };
        let value = wrap(
            context,
            object.class.name(),
            Selection {
                anchor,
                kind,
                filter,
            },
        )?;
        let _ = result.push(value);
    }
    Ok(Value::from(result))
}

pub fn get_namespaces(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    let recursive = args::bool(arguments, 0).unwrap_or(false);
    let mut result = PhpArray::new();
    if let Some(node) = state.selection.first() {
        node.document(|document| {
            let arena = &document.arena;
            let nodes =
                std::iter::once(node.id()).chain(arena.descendants(node.id()).take(if recursive {
                    usize::MAX
                } else {
                    0
                }));
            for id in nodes {
                let node = &arena[id];
                insert_used(&mut result, &node.name);
                for &attr in node.attributes() {
                    insert_used(&mut result, &arena[attr].name);
                }
            }
        });
    }
    Ok(Value::from(result))
}

fn insert_used(result: &mut PhpArray, name: &Name) {
    if let Some(uri) = &name.namespace
        && uri != XMLNS
    {
        insert(result, name.prefix.as_deref().unwrap_or_default(), uri);
    }
}

pub fn get_doc_namespaces(
    _: &mut dyn Context,
    this: This<'_>,
    arguments: &[Value],
) -> NativeResult {
    let Ok(state) = data(receiver(this)?) else {
        return Ok(Value::Bool(false));
    };
    let recursive = args::bool(arguments, 0).unwrap_or(false);
    let from_root = args::bool(arguments, 1).unwrap_or(true);
    let Some(node) = state.selection.first() else {
        return Ok(Value::Bool(false));
    };
    let mut result = PhpArray::new();
    node.document(|doc| {
        let arena = &doc.arena;
        let root = if from_root {
            arena
                .children(Arena::DOCUMENT)
                .find(|&id| arena[id].kind == Kind::Element)
        } else {
            Some(node.id())
        };
        if let Some(root) = root {
            for id in std::iter::once(root).chain(arena.descendants(root).take(if recursive {
                usize::MAX
            } else {
                0
            })) {
                for (prefix, uri) in declarations(arena, id) {
                    insert(&mut result, prefix, uri);
                }
            }
        }
    });
    Ok(Value::from(result))
}

fn insert(result: &mut PhpArray, key: &str, value: &str) {
    let key = phera_extension_api::ArrayKey::str(key);
    if !result.contains_key(&key) {
        result.insert(key, Value::from(value));
    }
}
