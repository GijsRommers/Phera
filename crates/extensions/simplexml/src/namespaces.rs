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

use crate::selection::{Filter, Selection, data, wrap};

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
        let Some(item) = node.select(item) else {
            continue;
        };
        let value = wrap(
            context,
            object.class.name(),
            Selection::node(item, Filter::Default),
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
    let state = data(receiver(this)?)?;
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
