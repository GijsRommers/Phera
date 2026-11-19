//! Assign virtual properties with PHP's scalar conversions.
//!
//! ```php
//! $document->formatOutput = true;
//! $element->textContent = 'plain text';
//! ```

use phera_extension_api::value::{to_bool, to_php_string};
use phera_extension_api::{Context, NativeError, NativePropertyMutation, Object, Value};

use crate::tree::{Kind, Name, Node};

use super::super::{attributes, dom_error};

pub(super) fn mutate(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    mutation: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    let location = super::location(object)?;
    let kind = location.shared.borrow().document.arena[location.id].kind;
    if !super::known(kind, name) {
        return Ok(false);
    }
    let NativePropertyMutation::Assign(value) = mutation else {
        return Err(NativeError::error(format!(
            "Cannot indirectly modify property {}::${name}",
            object.class.name()
        )));
    };
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
            let mut state = location.shared.borrow_mut();
            let settings = &mut state.document.settings;
            match name {
                "encoding" => settings.encoding = value,
                "documentURI" => settings.uri = value,
                _ => settings.version = value,
            }
        }
        "data" | "value" | "textContent" | "nodeValue" => {
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
            let value = stringify(context, value)?;
            let name = Name::new(if name == "id" { "id" } else { "class" });
            let mut state = location.shared.borrow_mut();
            attributes::set(&mut state.document.arena, location.id, name, value)?;
        }
        "prefix" => {
            let prefix = stringify(context, value)?;
            let mut state = location.shared.borrow_mut();
            let node = &mut state.document.arena[location.id];
            if node.name.namespace.is_some() {
                if !prefix.is_empty()
                    && (!crate::xml::names::valid(&prefix) || prefix.contains(':'))
                {
                    return Err(dom_error(14, "Namespace Error"));
                }
                node.name.prefix = (!prefix.is_empty()).then_some(prefix);
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
