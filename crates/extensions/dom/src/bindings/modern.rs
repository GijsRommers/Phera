//! Expose PHP's modern DOM class hierarchy over the shared document arena.
//!
//! ```php
//! $doc = Dom\HTMLDocument::createFromString('<p>Example</p>');
//! assert($doc->querySelector('p') instanceof Dom\HTMLElement);
//! ```

mod document;
mod element;
mod interfaces;
mod selectors;
mod tokens;

use phera_extension_api::{
    ClassEntry, ClassHooks, Context, MethodEntry, NativeClone, NativeError, NativePropertyAccess,
    NativePropertyMutation, NativePropertyRead, NativeResult, Object, Registry, Value,
};

use crate::html::XHTML;
use crate::tree::{Kind, Node};

use super::{Location, This, collection, location, node, properties};

pub(super) fn register(registry: &mut Registry) {
    registry.constant("Dom\\HTML_NO_DEFAULT_NS", 1_i64 << 31);
    node::register_modern(registry);
    interfaces::register(registry);
    document::register(registry);
    element::register(registry);
    tokens::register(registry);
    super::xpath::register_modern(registry);
    for (name, parent) in [
        ("Dom\\CharacterData", "Dom\\Node"),
        ("Dom\\Text", "Dom\\CharacterData"),
        ("Dom\\CDATASection", "Dom\\Text"),
        ("Dom\\Comment", "Dom\\CharacterData"),
        ("Dom\\ProcessingInstruction", "Dom\\CharacterData"),
        ("Dom\\DocumentType", "Dom\\Node"),
        ("Dom\\Entity", "Dom\\Node"),
        ("Dom\\EntityReference", "Dom\\Node"),
        ("Dom\\Notation", "Dom\\Node"),
    ] {
        registry.class(ClassEntry {
            parent: Some(parent),
            ..ClassEntry::class(name)
        });
    }
    registry.class(ClassEntry {
        parent: Some("Dom\\Node"),
        methods: const {
            &[MethodEntry::public("isId", &[], super::attributes::is_id).returning("bool")]
        },
        ..ClassEntry::class("Dom\\Attr")
    });
    for (name, legacy) in [
        ("Dom\\NodeList", "DOMNodeList"),
        ("Dom\\HTMLCollection", "DOMNamedNodeMap"),
        ("Dom\\NamedNodeMap", "DOMNamedNodeMap"),
        ("Dom\\DtdNamedNodeMap", "DOMNamedNodeMap"),
    ] {
        if let Some(entry) = registry.get_class(legacy).copied() {
            registry.class(ClassEntry { name, ..entry });
        }
    }
}

pub(super) fn class(node: &Node, html: bool) -> &'static str {
    match node.kind {
        Kind::Document if html => "Dom\\HTMLDocument",
        Kind::Document => "Dom\\XMLDocument",
        Kind::Element if node.name.namespace.as_deref() == Some(XHTML) => "Dom\\HTMLElement",
        Kind::Element => "Dom\\Element",
        Kind::Attribute | Kind::Namespace => "Dom\\Attr",
        Kind::Text => "Dom\\Text",
        Kind::Cdata => "Dom\\CDATASection",
        Kind::Comment => "Dom\\Comment",
        Kind::ProcessingInstruction => "Dom\\ProcessingInstruction",
        Kind::Doctype => "Dom\\DocumentType",
        Kind::Fragment => "Dom\\DocumentFragment",
        Kind::Entity => "Dom\\Entity",
        Kind::EntityReference => "Dom\\EntityReference",
        Kind::Notation => "Dom\\Notation",
    }
}

pub(super) const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::CopyWithHandle(node::clone_object)),
    property_access: Some(&NativePropertyAccess {
        read,
        mutate,
        string_property: Some(string_property),
    }),
    properties: Some(properties::view::view),
    ..ClassHooks::EMPTY
};

pub(super) fn html_element(location: &Location) -> bool {
    let state = location.shared.borrow();
    state.document.settings.html
        && state.document.arena[location.id].name.namespace.as_deref() == Some(XHTML)
}

pub(super) fn construct(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let name = this.map_or("Dom\\Node", |object| object.class.name());
    Err(NativeError::error(format!(
        "Cannot directly construct {name}"
    )))
}

fn read(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    mode: NativePropertyRead,
) -> Result<Option<Value>, NativeError> {
    let node = location(object)?;
    let kind = node.shared.borrow().document.arena[node.id].kind;
    let value = match (kind, name) {
        (Kind::Element | Kind::Document | Kind::Fragment, "children") => {
            collection::elements(context, node)?
        }
        (Kind::Element, "innerHTML") => element::inner_html(&node),
        (Kind::Element, "outerHTML") => element::outer_html(&node),
        (Kind::Element, "nodeName" | "tagName") if html_element(&node) => {
            let state = node.shared.borrow();
            Value::from(
                state.document.arena[node.id]
                    .name
                    .qualified()
                    .to_ascii_uppercase(),
            )
        }
        (Kind::Element | Kind::Attribute, "prefix") => {
            let state = node.shared.borrow();
            state.document.arena[node.id]
                .name
                .prefix
                .as_deref()
                .map(Value::from)
                .unwrap_or_default()
        }
        (
            Kind::Attribute,
            "parentNode" | "parentElement" | "firstChild" | "lastChild" | "previousSibling"
            | "nextSibling",
        ) => Value::Null,
        (Kind::Element, "classList") => tokens::wrap(context, node)?,
        (
            Kind::Document,
            "URL" | "documentURI" | "charset" | "characterSet" | "inputEncoding" | "body" | "head"
            | "title",
        ) => document::property(context, node, name)?,
        _ => return (properties::ACCESS.read)(context, object, name, mode),
    };
    Ok(Some(value))
}

fn string_property(
    object: &Object,
    name: &str,
) -> Option<phera_extension_api::NativeStringProperty> {
    let (declaring, nullable) = match name {
        "textContent" => ("Dom\\Node", true),
        "documentURI" if object.class.is_instance_of("Dom\\Document") => ("Dom\\Document", false),
        "innerHTML" | "className" | "id" if object.class.is_instance_of("Dom\\Element") => {
            ("Dom\\Element", false)
        }
        "data" if object.class.is_instance_of("Dom\\ProcessingInstruction") => {
            ("Dom\\ProcessingInstruction", false)
        }
        "data" if object.class.is_instance_of("Dom\\CharacterData") => {
            ("Dom\\CharacterData", false)
        }
        "value" if object.class.is_instance_of("Dom\\Attr") => ("Dom\\Attr", false),
        _ => return None,
    };
    Some(phera_extension_api::NativeStringProperty {
        declaring,
        nullable,
    })
}

fn mutate(
    context: &mut dyn Context,
    object: &Object,
    name: &str,
    update: NativePropertyMutation<'_>,
) -> Result<bool, NativeError> {
    let node = location(object)?;
    let kind = node.shared.borrow().document.arena[node.id].kind;
    if kind == Kind::Element
        && name == "innerHTML"
        && let NativePropertyMutation::Assign(value) = update
    {
        let text = phera_extension_api::value::to_php_string(value, context.diagnostics())?;
        element::set_inner_html(&node, &text.to_string_lossy())?;
        return Ok(true);
    }
    if matches!(name, "children" | "classList" | "URL" | "head") {
        return Err(NativeError::error(format!(
            "Cannot modify readonly property {}::${name}",
            object.class.name()
        )));
    }
    (properties::ACCESS.mutate)(context, object, name, update)
}
