//! Resolve DOM virtual properties against the live node tree.
//!
//! ```php
//! $text->nodeValue = 'updated';
//! echo $document->documentElement->textContent;
//! ```

mod write;

use phera_extension_api::{
    Context, NativeError, NativePropertyAccess, NativePropertyRead, NativeResult, Object, PhpArray,
    Value,
};

use crate::tree::{Arena, Kind, NodeId};

use super::{Location, attributes, collection, location, text};

pub(super) const ACCESS: NativePropertyAccess = NativePropertyAccess {
    read,
    mutate: write::mutate,
};

fn read(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    mode: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    let location = match location(object) {
        Ok(location) => location,
        Err(_)
            if matches!(
                mode,
                NativePropertyRead::Isset | NativePropertyRead::Quiet | NativePropertyRead::Exists
            ) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };
    let kind = location.shared.borrow().document.arena[location.id].kind;
    if !known(kind, name) {
        return Ok(None);
    }
    if matches!(mode, NativePropertyRead::Exists) {
        return Ok(Some(Value::Null));
    }
    value(context, &location, name).map(Some)
}

pub(super) fn known(kind: Kind, name: &str) -> bool {
    match name {
        "nodeName" | "nodeValue" | "nodeType" | "parentNode" | "parentElement" | "childNodes"
        | "firstChild" | "lastChild" | "previousSibling" | "nextSibling" | "attributes"
        | "isConnected" | "ownerDocument" | "namespaceURI" | "prefix" | "localName" | "baseURI"
        | "textContent" => true,
        "doctype"
        | "implementation"
        | "documentElement"
        | "actualEncoding"
        | "encoding"
        | "xmlEncoding"
        | "standalone"
        | "xmlStandalone"
        | "version"
        | "xmlVersion"
        | "strictErrorChecking"
        | "documentURI"
        | "config"
        | "formatOutput"
        | "validateOnParse"
        | "resolveExternals"
        | "preserveWhiteSpace"
        | "recover"
        | "substituteEntities" => kind == Kind::Document,
        "firstElementChild" | "lastElementChild" | "childElementCount" => {
            matches!(kind, Kind::Document | Kind::Element | Kind::Fragment)
        }
        "previousElementSibling" | "nextElementSibling" => {
            matches!(
                kind,
                Kind::Element | Kind::Text | Kind::Cdata | Kind::Comment
            )
        }
        "tagName" | "className" | "id" => kind == Kind::Element,
        "name" => matches!(kind, Kind::Attribute | Kind::Doctype),
        "value" | "specified" | "ownerElement" => kind == Kind::Attribute,
        "schemaTypeInfo" => matches!(kind, Kind::Attribute | Kind::Element),
        "data" => matches!(
            kind,
            Kind::Text | Kind::Cdata | Kind::Comment | Kind::ProcessingInstruction
        ),
        "length" => matches!(kind, Kind::Text | Kind::Cdata | Kind::Comment),
        "wholeText" => matches!(kind, Kind::Text | Kind::Cdata),
        "target" => kind == Kind::ProcessingInstruction,
        "publicId" | "systemId" | "internalSubset" | "entities" | "notations" => {
            kind == Kind::Doctype
        }
        _ => false,
    }
}

fn value(context: &mut dyn Context, location: &Location, property: &str) -> NativeResult {
    match property {
        "childNodes" => return collection::children(context, location.clone()),
        "attributes"
            if location.shared.borrow().document.arena[location.id].kind == Kind::Element =>
        {
            return collection::attributes(context, location.clone());
        }
        "implementation" => return context.create_object("DOMImplementation", PhpArray::new()),
        "entities" | "notations" => return collection::attributes(context, location.clone()),
        _ => {}
    }
    let state = location.shared.borrow();
    let arena = &state.document.arena;
    let settings = &state.document.settings;
    let id = location.id;
    let node = &arena[id];
    if let Some(element) = related(location, property) {
        drop(state);
        return location.wrap_optional(context, element);
    }

    let value = match property {
        "nodeName" | "tagName" => Value::from(node.name.qualified()),
        "name" | "target" => Value::from(node.name.local.clone()),
        "prefix" => Value::from(node.name.prefix.clone().unwrap_or_default()),
        "namespaceURI" => string(&node.name.namespace),
        "localName" if matches!(node.kind, Kind::Element | Kind::Attribute) => {
            Value::from(node.name.local.clone())
        }
        "nodeType" => Value::Int(match node.kind {
            Kind::Element => 1,
            Kind::Attribute => 2,
            Kind::Text => 3,
            Kind::Cdata => 4,
            Kind::EntityReference => 5,
            Kind::ProcessingInstruction => 7,
            Kind::Comment => 8,
            Kind::Document if settings.html => 13,
            Kind::Document => 9,
            Kind::Doctype => 10,
            Kind::Fragment => 11,
        }),
        "nodeValue" if matches!(node.kind, Kind::Document | Kind::Doctype | Kind::Fragment) => {
            Value::Null
        }
        "nodeValue" | "textContent" => Value::from(text(arena, id)),
        "data" | "value" => Value::from(node.value.clone()),
        "length" => Value::Int(node.value.chars().count() as i64),
        "wholeText" => Value::from(whole_text(arena, id)),
        "specified" => Value::Bool(true),
        "id" | "className" => {
            let name = if property == "id" { "id" } else { "class" };
            Value::from(
                attributes::find(arena, id, None, name)
                    .map_or_else(String::new, |id| arena[id].value.clone()),
            )
        }
        "isConnected" => Value::Bool(
            node.kind == Kind::Document
                || arena.ancestors(id).any(|parent| parent == Arena::DOCUMENT),
        ),
        "baseURI" | "documentURI" => string(&settings.uri),
        "encoding" | "xmlEncoding" | "actualEncoding" => string(&settings.encoding),
        "version" | "xmlVersion" => string(&settings.version),
        "standalone" | "xmlStandalone" => Value::Bool(settings.standalone.unwrap_or_default()),
        "strictErrorChecking" => Value::Bool(settings.strict),
        "formatOutput" => Value::Bool(settings.format),
        "validateOnParse" => Value::Bool(settings.validate_on_parse),
        "resolveExternals" => Value::Bool(settings.resolve_externals),
        "preserveWhiteSpace" => Value::Bool(settings.preserve_whitespace),
        "recover" => Value::Bool(settings.recover),
        "substituteEntities" => Value::Bool(settings.substitute_entities),
        "childElementCount" => Value::Int(elements(arena, id).count() as i64),
        "publicId" => Value::from(
            node.doctype
                .as_ref()
                .map_or("", |doctype| &doctype.public_id),
        ),
        "systemId" => Value::from(
            node.doctype
                .as_ref()
                .map_or("", |doctype| &doctype.system_id),
        ),
        "internalSubset" => node
            .doctype
            .as_ref()
            .map_or(Value::Null, |doctype| string(&doctype.internal_subset)),
        _ => Value::Null,
    };
    Ok(value)
}

fn related(location: &Location, property: &str) -> Option<Option<NodeId>> {
    let state = location.shared.borrow();
    let arena = &state.document.arena;
    let id = location.id;
    let node = &arena[id];
    Some(match property {
        "parentNode" => node.parent(),
        "ownerElement" | "parentElement" => {
            node.parent().filter(|&id| arena[id].kind == Kind::Element)
        }
        "ownerDocument" => (node.kind != Kind::Document && state.owned).then_some(Arena::DOCUMENT),
        "firstChild" => node.first_child(),
        "lastChild" => node.last_child(),
        "firstElementChild" | "documentElement" => elements(arena, id).next(),
        "lastElementChild" => elements(arena, id).last(),
        "previousSibling" => node.previous_sibling(),
        "nextSibling" => node.next_sibling(),
        "previousElementSibling" => {
            std::iter::successors(node.previous_sibling(), |&id| arena[id].previous_sibling())
                .find(|&id| arena[id].kind == Kind::Element)
        }
        "nextElementSibling" => {
            std::iter::successors(node.next_sibling(), |&id| arena[id].next_sibling())
                .find(|&id| arena[id].kind == Kind::Element)
        }
        "doctype" => arena
            .children(id)
            .find(|&id| arena[id].kind == Kind::Doctype),
        _ => return None,
    })
}

fn elements(arena: &Arena, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    arena
        .children(node)
        .filter(|&id| arena[id].kind == Kind::Element)
}

fn whole_text(arena: &Arena, node: NodeId) -> String {
    let is_text = |id: NodeId| matches!(arena[id].kind, Kind::Text | Kind::Cdata);
    let first = std::iter::successors(Some(node), |&id| arena[id].previous_sibling())
        .take_while(|&id| is_text(id))
        .last();
    let mut text = String::new();
    for id in
        std::iter::successors(first, |&id| arena[id].next_sibling()).take_while(|&id| is_text(id))
    {
        text.push_str(&arena[id].value);
    }
    text
}

fn string(value: &Option<String>) -> Value {
    value
        .as_ref()
        .map_or(Value::Null, |value| Value::from(value.as_str()))
}
