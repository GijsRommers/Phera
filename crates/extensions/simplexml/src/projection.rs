//! Project XML children and attributes for array casts, JSON and var_dump.
//!
//! ```php
//! var_dump((array) simplexml_load_string('<r id="1"><label>text</label></r>'));
//! ```

use std::rc::Rc;

use indexmap::IndexMap;
use phera_ext_dom::document::Document;
use phera_ext_dom::shared::SharedNode;
use phera_ext_dom::tree::{Arena, Kind, NodeId};
use phera_extension_api::{
    ArrayKey, Context, NativeError, NativeResult, Object, PhpArray, PropertyPurpose, Value,
    value::{NoDiagnostics, ScalarCast, to_float, to_int},
};

use crate::methods::{This, receiver};
use crate::selection::{Selected, Selection, attributes, data, install, value, wrap};

pub fn cast(object: &Object, kind: ScalarCast) -> Option<Value> {
    let state = data(object).ok()?;
    let selection = &state.selection;
    match kind {
        ScalarCast::Bool => Some(Value::Bool(selection.is_truthy())),
        ScalarCast::Int => {
            let text = Value::from(selection.value());
            to_int(&text, &mut NoDiagnostics).ok().map(Value::Int)
        }
        ScalarCast::Float => {
            let text = Value::from(selection.value());
            to_float(&text, &mut NoDiagnostics).ok().map(Value::Float)
        }
    }
}

pub fn properties(
    context: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    if matches!(purpose, PropertyPurpose::Serialize) {
        return Err(NativeError::throw(
            "Exception",
            "Serialization of 'SimpleXMLElement' is not allowed",
        ));
    }
    let state = data(object)?;
    let selection = &state.selection;
    let mut result = PhpArray::new();
    match &selection.kind {
        Selected::Attributes => {
            insert_attrs(selection, selection.anchor.id(), &mut result);
        }
        Selected::Node | Selected::Children(Some(_)) => {
            let Some(node) = selection.first() else {
                return Ok(result);
            };
            insert_attrs(selection, node.id(), &mut result);
            if named_list(selection, &node) {
                for id in selection.items() {
                    let item = project_node(context, object, selection, id)?;
                    let _ = result.push(item);
                }
                return Ok(result);
            }
            let (rows, text) = node.document(|doc| {
                let rows = projected_children(&doc.arena, node.id(), selection);
                let text = scalar_property(&doc.arena, node.id());
                (rows, text)
            });
            insert_rows(context, object, selection, rows, &mut result)?;
            if let Some(text) = text {
                let _ = result.push(Value::from(text));
            }
        }
        Selected::Children(None) => {
            if matches!(purpose, PropertyPurpose::Debug) {
                insert_attrs(selection, selection.anchor.id(), &mut result);
            }
            let rows = selection
                .anchor
                .document(|doc| group_rows(&doc.arena, selection.items()));
            insert_rows(context, object, selection, rows, &mut result)?;
        }
    }
    Ok(result)
}

fn named_list(selection: &Selection, node: &SharedNode) -> bool {
    if !matches!(selection.kind, Selected::Children(Some(_))) {
        return false;
    }
    node.document(|doc| {
        let arena = &doc.arena;
        let node = &arena[node.id()];
        node.next_sibling().is_some()
            && node
                .parent()
                .is_some_and(|parent| arena[parent].first_child() != arena[parent].last_child())
            && node.first_child().is_some_and(|first| {
                arena[first].next_sibling().is_none() && arena[first].first_child().is_none()
            })
    })
}

fn insert_attrs(selection: &Selection, node: NodeId, result: &mut PhpArray) {
    let data = selection.anchor.document(|doc| {
        let arena = &doc.arena;
        attributes(arena, node, &selection.filter, None)
            .into_iter()
            .map(|id| (arena[id].name.local.clone(), arena[id].value.clone()))
            .collect::<Vec<_>>()
    });
    let mut map = PhpArray::new();
    for (name, value) in data {
        map.insert(ArrayKey::str(name), Value::from(value));
    }
    if !map.is_empty() {
        result.insert(ArrayKey::str("@attributes"), Value::from(map));
    }
}

type Rows = IndexMap<String, Vec<NodeId>>;

fn projected_children(arena: &Arena, node: NodeId, selection: &Selection) -> Rows {
    group_rows(
        arena,
        arena.children(node).filter(|&child| {
            let child = &arena[child];
            matches!(
                child.kind,
                Kind::Element | Kind::Comment | Kind::ProcessingInstruction
            ) && selection.filter.matches(&child.name)
        }),
    )
}

fn group_rows(arena: &Arena, nodes: impl IntoIterator<Item = NodeId>) -> Rows {
    let mut rows = Rows::new();
    for id in nodes {
        let node = &arena[id];
        let name = if node.kind == Kind::Comment {
            "comment"
        } else {
            &node.name.local
        };
        rows.entry(name.to_owned()).or_default().push(id);
    }
    rows
}

fn insert_rows(
    context: &mut dyn Context,
    object: &Object,
    selection: &Selection,
    rows: Rows,
    result: &mut PhpArray,
) -> Result<(), NativeError> {
    for (name, row) in rows {
        let entry = match row.as_slice() {
            [item] => project_node(context, object, selection, *item)?,
            _ => {
                let mut values = PhpArray::new();
                for node in row {
                    let _ = values.push(project_node(context, object, selection, node)?);
                }
                Value::from(values)
            }
        };
        result.insert(ArrayKey::str(name), entry);
    }
    Ok(())
}

fn project_node(
    context: &mut dyn Context,
    object: &Object,
    selection: &Selection,
    id: NodeId,
) -> NativeResult {
    let text = selection.anchor.document(|doc| {
        let arena = &doc.arena;
        arena[id].first_child().and_then(|child| {
            let first = &arena[child];
            (first.kind == Kind::Text && !first.value.trim().is_empty()).then(|| value(arena, id))
        })
    });
    if let Some(text) = text {
        return Ok(Value::from(text));
    }
    let Some(node) = selection.anchor.select(id) else {
        return Ok(Value::Null);
    };
    wrap(
        context,
        object.class.name(),
        Selection::node(node, selection.filter.clone()),
    )
}

fn scalar_property(arena: &Arena, id: NodeId) -> Option<String> {
    let node = &arena[id];
    if node.kind == Kind::Attribute {
        return Some(node.value.clone());
    }
    let child = &arena[node.first_child()?];
    (child.kind == Kind::Text && child.next_sibling().is_none() && !child.value.trim().is_empty())
        .then(|| child.value.clone())
}

pub fn debug(context: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::from(properties(
        context,
        receiver(this)?,
        PropertyPurpose::Debug,
    )?))
}

pub fn clone(source: &Rc<Object>, destination: &Object) -> Result<(), NativeError> {
    let state = data(source)?;
    let mut selection = state.selection.clone();
    let copied = selection.anchor.document(|doc| Document {
        arena: doc.arena.clone(),
        settings: doc.settings.clone(),
    });
    let anchor = selection.anchor.id();
    if let Some(node) = SharedNode::root(copied).and_then(|root| root.select(anchor)) {
        selection.anchor = node;
        install(destination, selection);
    }
    Ok(())
}
