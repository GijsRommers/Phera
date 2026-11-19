//! Read typed XMLReader properties from the current cursor.
//!
//! ```php
//! $reader->read();
//! echo $reader->name, ' has ', $reader->attributeCount, ' attributes';
//! ```

use phera_ext_dom::tree::{Arena, Kind, NodeId};
use phera_extension_api::{
    ArrayKey, Context, NativeError, NativePropertyAccess, NativePropertyMutation,
    NativePropertyRead, Object, PhpArray, PropertyEntry, PropertyPurpose, PropertyType, Value,
    Visibility,
};

use crate::cursor::Position;
use crate::state::{State, data};

pub const PROPERTIES: &[PropertyEntry] = &[
    property("attributeCount", PropertyType::Int),
    property("baseURI", PropertyType::String),
    property("depth", PropertyType::Int),
    property("hasAttributes", PropertyType::Bool),
    property("hasValue", PropertyType::Bool),
    property("isDefault", PropertyType::Bool),
    property("isEmptyElement", PropertyType::Bool),
    property("localName", PropertyType::String),
    property("name", PropertyType::String),
    property("namespaceURI", PropertyType::String),
    property("nodeType", PropertyType::Int),
    property("prefix", PropertyType::String),
    property("value", PropertyType::String),
    property("xmlLang", PropertyType::String),
];

const fn property(name: &'static str, ty: PropertyType) -> PropertyEntry {
    PropertyEntry::typed(name, ty, Visibility::Public)
}

pub const ACCESS: NativePropertyAccess = NativePropertyAccess { read, mutate };

fn read(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    mode: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    let Some(property) = PROPERTIES.iter().find(|entry| entry.name == name) else {
        return Ok(None);
    };
    if matches!(mode, NativePropertyRead::Exists) {
        return Ok(Some(Value::Bool(true)));
    }
    let state = data(object);
    let reader = state.as_ref().map(|state| state.borrow());
    Ok(Some(get_property(reader.as_deref(), property)?))
}

fn mutate(
    _: &mut dyn Context,
    object: &Object,
    name: &str,
    operation: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    if !PROPERTIES.iter().any(|entry| entry.name == name) {
        return Ok(false);
    }
    let class = object.class.name();
    let message = match operation {
        NativePropertyMutation::Unset => format!("Cannot unset {class}::${name}"),
        _ => format!("Cannot modify readonly property {class}::${name}"),
    };
    Err(NativeError::error(message))
}

pub fn project(
    _: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let mut values = object.to_array();
    if !matches!(purpose, PropertyPurpose::Debug) {
        return Ok(values);
    }
    let state = data(object);
    let state = state.as_ref().map(|state| state.borrow());
    let before = state
        .as_ref()
        .is_some_and(|state| matches!(state.position, Position::Before));
    for property in PROPERTIES {
        if before && property.name == "isEmptyElement" {
            continue;
        }
        let read = if before { None } else { state.as_deref() };
        if let Ok(value) = get_property(read, property) {
            values.insert(ArrayKey::str(property.name), value);
        }
    }
    Ok(values)
}

fn get_property(state: Option<&State>, property: &PropertyEntry) -> Result<Value, NativeError> {
    if let Some(state) = state
        && matches!(state.position, Position::Before)
    {
        return Err(NativeError::error(
            "Failed to read property because no XML data has been read yet",
        ));
    }
    let Some((state, id, tree)) =
        state.and_then(|state| Some((state, state.position.id()?, state.tree.as_ref()?)))
    else {
        return Ok(match property.ty {
            Some(PropertyType::Bool) => Value::Bool(false),
            Some(PropertyType::Int) => Value::Int(0),
            _ => Value::from(""),
        });
    };
    let read = tree.document(|doc| {
        let arena = &doc.arena;
        let node = &arena[id];
        let end = matches!(state.position, Position::End { .. });
        let node_type = node_type(arena, state.position, id);
        let name = match node_type {
            3 | 13 | 14 => "#text".to_owned(),
            4 => "#cdata-section".to_owned(),
            8 => "#comment".to_owned(),
            _ => node.name.qualified(),
        };
        let value = if has_value(node_type) {
            &node.value
        } else {
            ""
        };
        match property.name {
            "attributeCount" => Value::Int(if end {
                0
            } else {
                node.attributes().len() as i64
            }),
            "baseURI" => Value::from(base_uri(arena, id, &state.uri)),
            "depth" => Value::Int(state.position.depth() as i64),
            "hasAttributes" => Value::Bool(!end && !node.attributes().is_empty()),
            "hasValue" => Value::Bool(has_value(node_type)),
            "isDefault" => Value::Bool(node_type == 2 && node.default_attribute),
            "isEmptyElement" => Value::Bool(node_type == 1 && node.empty_element),
            "localName" => Value::from(match node_type {
                1 | 2 | 15 => node.name.local.clone(),
                _ => name,
            }),
            "name" => Value::from(name),
            "namespaceURI" => Value::from(node.name.namespace.as_deref().unwrap_or_default()),
            "nodeType" => Value::Int(node_type),
            "prefix" => Value::from(node.name.prefix.as_deref().unwrap_or_default()),
            "value" => Value::from(value),
            "xmlLang" => Value::from(xml_attribute(arena, id, "lang").unwrap_or_default()),
            _ => Value::Null,
        }
    });
    Ok(read)
}

pub fn node_type(arena: &Arena, position: Position, id: NodeId) -> i64 {
    if matches!(position, Position::End { .. }) {
        return 15;
    }
    let node = &arena[id];
    match node.kind {
        Kind::Element => 1,
        Kind::Attribute | Kind::Namespace => 2,
        Kind::Text
            if node
                .value
                .chars()
                .all(|c| matches!(c, ' ' | '\t' | '\r' | '\n')) =>
        {
            14
        }
        Kind::Text => 3,
        Kind::Cdata => 4,
        Kind::EntityReference => 5,
        Kind::Entity => 6,
        Kind::ProcessingInstruction => 7,
        Kind::Comment => 8,
        Kind::Document => 9,
        Kind::Doctype => 10,
        Kind::Fragment => 11,
        Kind::Notation => 12,
    }
}

fn has_value(node_type: i64) -> bool {
    matches!(node_type, 2..=4 | 7 | 8 | 13 | 14 | 17)
}

fn xml_attribute<'a>(arena: &'a Arena, node: NodeId, name: &str) -> Option<&'a str> {
    std::iter::once(node)
        .chain(arena.ancestors(node))
        .find_map(|element| {
            arena[element].attributes().iter().find_map(|&attribute| {
                let attr = &arena[attribute];
                (attr.name.local == name
                    && attr.name.namespace.as_deref()
                        == Some("http://www.w3.org/XML/1998/namespace"))
                .then_some(attr.value.as_str())
            })
        })
}

fn base_uri(arena: &Arena, id: NodeId, uri: &str) -> String {
    let Some(base) = xml_attribute(arena, id, "base") else {
        return uri.to_owned();
    };
    if let Ok(url) = url::Url::parse(base) {
        return url.into();
    }
    url::Url::parse(uri)
        .ok()
        .and_then(|url| url.join(base).ok())
        .map_or_else(|| base.to_owned(), Into::into)
}
