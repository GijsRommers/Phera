//! Expose legacy node properties in PHP's debug view.
//!
//! ```php
//! var_dump($document->documentElement); // object-valued entries are omitted
//! ```

use crate::tree::Kind;
use phera_extension_api::{
    ArrayKey, Context, NativeError, Object, PhpArray, PropertyPurpose, Value,
};

use super::{location, value};

const NODE: &[&str] = &[
    "nodeName",
    "nodeValue",
    "nodeType",
    "parentNode",
    "parentElement",
    "childNodes",
    "firstChild",
    "lastChild",
    "previousSibling",
    "nextSibling",
    "attributes",
    "isConnected",
    "ownerDocument",
    "namespaceURI",
    "prefix",
    "localName",
    "baseURI",
    "textContent",
];
const DOCUMENT: &[&str] = &[
    "doctype",
    "implementation",
    "documentElement",
    "actualEncoding",
    "encoding",
    "xmlEncoding",
    "standalone",
    "xmlStandalone",
    "version",
    "xmlVersion",
    "strictErrorChecking",
    "documentURI",
    "config",
    "formatOutput",
    "validateOnParse",
    "resolveExternals",
    "preserveWhiteSpace",
    "recover",
    "substituteEntities",
    "firstElementChild",
    "lastElementChild",
    "childElementCount",
];
const ELEMENT: &[&str] = &[
    "tagName",
    "className",
    "id",
    "schemaTypeInfo",
    "firstElementChild",
    "lastElementChild",
    "childElementCount",
    "previousElementSibling",
    "nextElementSibling",
];
const ATTRIBUTE: &[&str] = &[
    "name",
    "specified",
    "value",
    "ownerElement",
    "schemaTypeInfo",
];
const DATA: &[&str] = &[
    "data",
    "length",
    "previousElementSibling",
    "nextElementSibling",
];

pub(in crate::bindings) fn view(
    context: &mut dyn Context,
    object: &Object,
    purpose: PropertyPurpose,
) -> Result<PhpArray, NativeError> {
    let mut table = object.to_array();
    if purpose != PropertyPurpose::Debug {
        return Ok(table);
    }
    let Ok(location) = location(object) else {
        return Ok(table);
    };
    let (kind, owned) = {
        let state = location.shared.borrow();
        (state.document.arena[location.id].kind, state.owned)
    };
    let (primary, inherited): (&[&str], &[&str]) = match kind {
        Kind::Document => (DOCUMENT, &[]),
        Kind::Element => (ELEMENT, &[]),
        Kind::Attribute => (ATTRIBUTE, &[]),
        Kind::Text | Kind::Cdata => (&["wholeText"], DATA),
        Kind::Comment => (DATA, &[]),
        Kind::Fragment => (
            &["firstElementChild", "lastElementChild", "childElementCount"],
            &[],
        ),
        Kind::Doctype => (
            &[
                "name",
                "entities",
                "notations",
                "publicId",
                "systemId",
                "internalSubset",
            ],
            &[],
        ),
        Kind::ProcessingInstruction => (&["target", "data"], &[]),
        Kind::EntityReference => (&[], &[]),
        Kind::Entity => (
            &[
                "publicId",
                "systemId",
                "notationName",
                "actualEncoding",
                "encoding",
                "version",
            ],
            &[],
        ),
        Kind::Notation => (&["publicId", "systemId"], &[]),
    };
    for &name in primary.iter().chain(inherited).chain(NODE) {
        if name == "ownerDocument" && !owned {
            continue;
        }
        let value = match name {
            "childNodes" | "implementation" | "entities" | "notations" => {
                Value::from("(object value omitted)")
            }
            "attributes" if kind == Kind::Element => Value::from("(object value omitted)"),
            _ => match super::related(&location, name) {
                Some(Some(_)) => Value::from("(object value omitted)"),
                Some(None) => Value::Null,
                None => value(context, &location, name)?,
            },
        };
        table.insert(ArrayKey::str(name), value);
    }
    Ok(table)
}
