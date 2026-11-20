//! Expose SimpleXML values, child selections and XML output.
//!
//! ```php
//! $node = $xml->addChild('item', 'value');
//! $node->addAttribute('active', 'yes');
//! echo $node->asXML();
//! ```

use std::rc::Rc;

use phera_ext_dom::serialize::{self, Options};
use phera_ext_dom::shared::SharedNode;
use phera_ext_dom::tree::{Arena, Kind, Name, Node};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};

use crate::load::{resolve_file, selection_filter};
use crate::properties;
use crate::selection::{Filter, Selected, Selection, data, wrap};

pub type This<'a> = Option<&'a Rc<Object>>;

pub fn receiver(this: This<'_>) -> Result<&Object, NativeError> {
    this.map(Rc::as_ref)
        .ok_or_else(|| NativeError::error("SimpleXMLElement is not properly initialized"))
}

pub fn name(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::from(data(receiver(this)?)?.selection.name()))
}

pub fn stringify(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::from(data(receiver(this)?)?.selection.value()))
}

pub fn count(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Int(
        data(receiver(this)?)?.selection.items().len() as i64
    ))
}

pub fn children(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    select(
        context,
        receiver(this)?,
        arguments,
        Selected::Children(None),
    )
}

pub fn attributes(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    select(
        context,
        receiver(this)?,
        arguments,
        Selected::Attributes(None),
    )
}

fn select(
    context: &mut dyn Context,
    object: &Object,
    arguments: &[Value],
    kind: Selected,
) -> NativeResult {
    let state = data(object)?;
    if let Some(anchor) = state.selection.first()
        && anchor.document(|doc| doc.arena[anchor.id()].kind == Kind::Element)
    {
        let filter = selection_filter(arguments, 0);
        wrap(
            context,
            object.class.name(),
            Selection {
                anchor,
                kind,
                filter,
            },
        )
    } else {
        Ok(Value::Null)
    }
}

pub fn save(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let state = data(receiver(this.as_ref())?)?;
        let Some(node) = state.selection.first() else {
            return Ok(Value::Bool(false));
        };
        let content = node.document(|doc| {
            let root = node.id();
            let root = if doc.arena[root].kind == Kind::Element
                && doc.arena[root].parent() == Some(Arena::DOCUMENT)
            {
                Arena::DOCUMENT
            } else {
                root
            };
            serialize::save(doc, root, Options::default())
        });
        let Some(filename) = args::string(&arguments, 0) else {
            return Ok(Value::from(content));
        };
        let Some(path) = resolve_file(
            context,
            filename.as_bytes(),
            "SimpleXMLElement::asXML",
            "filename",
        )?
        else {
            return Ok(Value::Bool(false));
        };
        let written = tokio::fs::write(path, content).await.is_ok();
        if !written {
            let display = filename.to_string_lossy();
            phera_ext_dom::shared::publish_xml_file_error(
                context,
                "SimpleXMLElement::asXML",
                &display,
            );
        }
        Ok(Value::Bool(written))
    })
}

pub fn add_child(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let state = data(object)?;
    let name = qualified_name("addChild", args)?;
    if matches!(state.selection.kind, Selected::Attributes(_)) {
        context.warning("SimpleXMLElement::addChild(): Cannot add element to attributes".into());
        return Ok(Value::Null);
    }
    let Some(owner) = state.selection.first() else {
        context.warning("SimpleXMLElement::addChild(): Cannot add child. Parent is not a permanent member of the XML tree".into());
        return Ok(Value::Null);
    };
    let filter_result = name.prefix.is_some() || args::string(args, 2).is_some();
    let result = owner.document_mut(|doc| {
        let arena = &mut doc.arena;
        if arena[owner.id()].kind != Kind::Element {
            return None;
        }
        let mut name = name;
        if name.namespace.is_none() && args::string(args, 2).is_none() {
            let parent = &arena[owner.id()].name;
            name.namespace.clone_from(&parent.namespace);
            name.prefix.clone_from(&parent.prefix);
        } else if let Some(uri) = &name.namespace
            && let Some(prefix) = crate::namespaces::prefix_for_uri(arena, owner.id(), uri, false)
        {
            name.prefix = (!prefix.is_empty()).then_some(prefix);
        } else if name.namespace.is_none() {
            name.prefix = None;
        }
        let prefix = name.prefix.clone().filter(|_| filter_result);
        let id = arena.allocate(Node::new(Kind::Element, name, ""));
        arena
            .insert(owner.id(), id, None)
            .ok()
            .map(|_| (id, prefix))
    });
    let Some((id, prefix)) = result else {
        return Ok(Value::Null);
    };
    let Some(node) = owner.select(id) else {
        return Ok(Value::Null);
    };
    declare_namespace(&node, &node);
    if let Some(value) = args::string(args, 1) {
        node.assign_text(value.to_string_lossy());
    }
    wrap(
        context,
        object.class.name(),
        Selection::node(node, Filter::new(prefix.as_deref(), true)),
    )
}

pub fn add_attribute(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let state = data(receiver(this)?)?;
    let mut name = qualified_name("addAttribute", args)?;
    let Some(owner) = state.selection.attribute_owner() else {
        context.warning("SimpleXMLElement::addAttribute(): Unable to locate parent Element".into());
        return Ok(Value::Null);
    };
    if name.namespace.is_some() && name.prefix.is_none() {
        context.warning(
            "SimpleXMLElement::addAttribute(): Attribute requires prefix for namespace".into(),
        );
        return Ok(Value::Null);
    }
    if name.namespace.is_none() {
        name.prefix = None;
    } else if let Some(prefix) = owner.document(|doc| {
        crate::namespaces::prefix_for_uri(&doc.arena, owner.id(), name.namespace.as_deref()?, true)
    }) {
        name.prefix = Some(prefix);
    }
    if owner.document(|doc| {
        let arena = &doc.arena;
        arena[owner.id()].attributes().iter().any(|attr| {
            let current = &arena[*attr].name;
            current.local == name.local && current.namespace == name.namespace
        })
    }) {
        context.warning("SimpleXMLElement::addAttribute(): Attribute already exists".into());
        return Ok(Value::Null);
    }
    let value = String::from_utf8_lossy(args::bytes(args, 1)).into_owned();
    if let Some(attribute) =
        properties::add_attribute(&owner, name, value)?.and_then(|id| owner.select(id))
    {
        declare_namespace(&owner, &attribute);
    }
    Ok(Value::Null)
}

fn qualified_name(method: &str, arguments: &[Value]) -> Result<Name, NativeError> {
    let qualified = String::from_utf8_lossy(args::bytes(arguments, 0));
    if qualified.is_empty() {
        return Err(NativeError::value_error(format!(
            "SimpleXMLElement::{method}(): Argument #1 ($qualifiedName) must not be empty",
        )));
    }
    let mut name = match qualified.split_once(':') {
        Some((prefix, local)) => Name {
            local: local.to_owned(),
            prefix: Some(prefix.to_owned()),
            namespace: None,
        },
        None => Name::new(qualified.as_ref()),
    };
    name.namespace = args::string(arguments, 2)
        .map(phera_extension_api::PhpString::to_string_lossy)
        .filter(|namespace| !namespace.is_empty());
    Ok(name)
}

fn declare_namespace(owner: &SharedNode, declared: &SharedNode) {
    let declaration = declared.document(|doc| doc.arena[declared.id()].name.clone());
    owner.document_mut(|doc| {
        let arena = &mut doc.arena;
        let prefix = declaration.prefix.as_deref().unwrap_or_default();
        let Some(namespace) = declaration.namespace else {
            return;
        };
        if crate::namespaces::lookup(arena, owner.id(), prefix).as_deref() == Some(&namespace) {
            return;
        }
        let name = if prefix.is_empty() {
            Name {
                local: "xmlns".into(),
                prefix: None,
                namespace: Some(XMLNS.into()),
            }
        } else {
            Name {
                local: prefix.into(),
                prefix: Some("xmlns".into()),
                namespace: Some(XMLNS.into()),
            }
        };
        let id = arena.allocate(Node::new(Kind::Attribute, name, namespace));
        let _ = arena.set_attribute(owner.id(), id, None);
    });
}

const XMLNS: &str = "http://www.w3.org/2000/xmlns/";
