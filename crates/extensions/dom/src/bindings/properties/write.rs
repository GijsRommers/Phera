//! Assign virtual properties with PHP's scalar conversions.
//!
//! ```php
//! $document->formatOutput = true;
//! $element->textContent = 'plain text';
//! ```

use phera_extension_api::value::{to_bool, to_php_string};
use phera_extension_api::{Context, NativeError, NativePropertyMutation, Object, Value};

use crate::tree::{Kind, Name, Node};
use crate::xml::names::{XML, XMLNS};

use super::super::{attributes, dom_error, namespace};

pub(super) fn string_property(
    object: &Object,
    name: &str,
) -> Option<phera_extension_api::NativeStringProperty> {
    let (declaring, nullable) = match name {
        "nodeValue" => ("DOMNode", true),
        "textContent" | "prefix" => ("DOMNode", false),
        "encoding" | "version" | "xmlVersion" | "documentURI"
            if object.class.is_instance_of("DOMDocument") =>
        {
            ("DOMDocument", true)
        }
        "data" if object.class.is_instance_of("DOMProcessingInstruction") => {
            ("DOMProcessingInstruction", false)
        }
        "data" if object.class.is_instance_of("DOMCharacterData") => ("DOMCharacterData", false),
        "value" if object.class.is_instance_of("DOMAttr") => ("DOMAttr", false),
        _ => return None,
    };
    Some(phera_extension_api::NativeStringProperty {
        declaring,
        nullable,
    })
}

pub(super) fn mutate(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    mutation: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    let location = super::location(object);
    let kind = location.as_ref().map_or_else(
        |_| super::uninitialized_kind(object),
        |node| node.shared.borrow().document.arena[node.id].kind,
    );
    if !super::known(kind, name) {
        return Ok(false);
    }
    if matches!(mutation, NativePropertyMutation::Unset) {
        return Err(NativeError::error(format!(
            "Cannot unset {}::${name}",
            object.class.name(),
        )));
    }
    let NativePropertyMutation::Assign(value) = mutation else {
        return Err(NativeError::error(format!(
            "Cannot indirectly modify property {}::${name}",
            object.class.name()
        )));
    };
    if kind == Kind::Entity && matches!(name, "actualEncoding" | "encoding" | "version") {
        return Err(NativeError::error(format!(
            "Cannot modify readonly property {}::${name}",
            object.class.name(),
        )));
    }
    let location = location?;
    match name {
        "strictErrorChecking"
        | "formatOutput"
        | "preserveWhiteSpace"
        | "recover"
        | "resolveExternals"
        | "substituteEntities"
        | "validateOnParse"
        | "standalone"
        | "xmlStandalone" => {
            let value = to_bool(value);
            let mut state = location.shared.borrow_mut();
            let settings = &mut state.document.settings;
            match name {
                "strictErrorChecking" => settings.strict = value,
                "formatOutput" => settings.format = value,
                "preserveWhiteSpace" => settings.preserve_whitespace = value,
                "recover" => settings.recover = value,
                "resolveExternals" => settings.resolve_externals = value,
                "substituteEntities" => settings.substitute_entities = value,
                "validateOnParse" => settings.validate_on_parse = value,
                _ => settings.standalone = Some(value),
            }
        }
        "encoding" | "version" | "xmlVersion" | "documentURI" => {
            let value = if matches!(value, Value::Null) {
                None
            } else {
                Some(stringify(context, value)?)
            };
            if name == "encoding" && !value.as_deref().is_some_and(crate::xml::supported_encoding) {
                return Err(NativeError::value_error("Invalid document encoding"));
            }
            let mut state = location.shared.borrow_mut();
            let settings = &mut state.document.settings;
            match name {
                "encoding" => settings.encoding = value,
                "documentURI" => settings.uri = value,
                _ => settings.version = value,
            }
        }
        "data" | "value" | "textContent" | "nodeValue" => {
            let dereferenced = value.deref();
            let invalid = match &*dereferenced {
                Value::Array(_) | Value::Resource(_) => true,
                Value::Object(object) => !object.class.is_instance_of("Stringable"),
                _ => false,
            };
            if invalid {
                let declaring = match name {
                    "data" if kind == Kind::ProcessingInstruction => "DOMProcessingInstruction",
                    "data" => "DOMCharacterData",
                    "value" => "DOMAttr",
                    _ => "DOMNode",
                };
                let expected = if matches!(name, "data" | "value") {
                    "string"
                } else {
                    "?string"
                };
                return Err(NativeError::type_error(format!(
                    "Cannot assign {} to property {declaring}::${name} of type {expected}",
                    value.type_name(),
                )));
            }
            let value = if matches!(value, Value::Null) {
                String::new()
            } else {
                stringify(context, value)?
            };
            let mut state = location.shared.borrow_mut();
            let arena = &mut state.document.arena;
            let id = location.id;
            match kind {
                Kind::Element | Kind::Fragment => {
                    while let Some(child) = arena[id].first_child() {
                        arena.detach(child);
                    }
                    if !value.is_empty() {
                        let child =
                            arena.allocate(Node::new(Kind::Text, Name::new("#text"), value));
                        let _ = arena.insert(id, child, None);
                    }
                }
                Kind::Document | Kind::Doctype => {}
                Kind::Attribute => arena.assign_attribute(id, value),
                _ => {
                    arena[id].value = value;
                    arena.refresh_parent_attribute(id);
                }
            }
        }
        "className" | "id" => {
            let mut value = stringify(context, value)?;
            if let Some(end) = value.find('\0') {
                value.truncate(end);
            }
            let name = Name::new(if name == "id" { "id" } else { "class" });
            let mut state = location.shared.borrow_mut();
            let html = state.document.settings.html;
            let arena = &mut state.document.arena;
            let attribute = attributes::set(arena, location.id, name, value, html)?;
            if arena[attribute].name.local == "id" {
                arena[attribute].is_id = true;
            }
        }
        "prefix" => {
            let prefix = stringify(context, value)?;
            let prefix = prefix.split('\0').next().unwrap_or_default();
            let mut state = location.shared.borrow_mut();
            let arena = &mut state.document.arena;
            let node = &arena[location.id];
            if let Some(uri) = node.name.namespace.clone() {
                if (prefix == "xml" && uri != XML) || (prefix == "xmlns" && uri != XMLNS) {
                    return Err(dom_error(14, "Namespace Error"));
                }
                let owner = if node.kind == Kind::Element {
                    Some(location.id)
                } else {
                    node.parent()
                };
                if let Some(owner) = owner
                    && uri != XML
                    && uri != XMLNS
                {
                    let local = if prefix.is_empty() { "xmlns" } else { prefix };
                    if let Some(declaration) =
                        attributes::find(arena, owner, Some(Some(XMLNS)), local)
                    {
                        if arena[declaration].value != uri {
                            return Err(dom_error(14, "Namespace Error"));
                        }
                    } else {
                        namespace::declare(arena, owner, prefix, &uri);
                    }
                }
                arena[location.id].name.prefix = (!prefix.is_empty()).then(|| prefix.to_owned());
            }
        }
        _ => {
            return Err(NativeError::error(format!(
                "Cannot modify readonly property {}::${name}",
                object.class.name()
            )));
        }
    }
    Ok(true)
}

fn stringify(context: &mut dyn Context, value: &Value) -> Result<String, NativeError> {
    let value = to_php_string(value, context.diagnostics())?;
    Ok(String::from_utf8_lossy(value.as_bytes()).into_owned())
}
