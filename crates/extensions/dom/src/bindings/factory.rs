//! Construct detached nodes and documents with PHP's DOM factories.
//!
//! ```php
//! $node = $document->createElementNS('urn:foo', 'f:item');
//! $document->appendChild($node);
//! ```

mod implementation;
pub(super) use implementation::{has_feature, register};

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::document::Document;
use crate::tree::{Arena, Kind, Name, Node, NodeId};
use crate::xml::names::{self, XML, XMLNS};

use super::{Location, This, dom_error, location, receiver, this_node};

pub(super) fn argument(arguments: &[Value], index: usize) -> Option<String> {
    args::string(arguments, index)
        .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
}

pub(super) fn namespace(arguments: &[Value], index: usize) -> Option<String> {
    argument(arguments, index).filter(|uri| !uri.is_empty())
}

pub(super) fn create_element(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    create(
        context,
        this,
        "DOMDocument::createElement",
        element(argument(args, 0), argument(args, 1), None, false),
    )
}

pub(super) fn create_element_ns(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    create(
        context,
        this,
        "DOMDocument::createElementNS",
        element(
            argument(args, 1),
            argument(args, 2),
            namespace(args, 0),
            true,
        ),
    )
}

pub(super) fn create_attribute(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let name = checked_name(argument(args, 0).unwrap_or_default(), None, false);
    create(
        context,
        this,
        "DOMDocument::createAttribute",
        name.map(|name| Node::new(Kind::Attribute, name, "")),
    )
}

pub(super) fn create_attribute_ns(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let document = this_node(this)?;
    let mut state = document.shared.borrow_mut();
    let arena = &mut state.document.arena;
    let root = arena
        .children(Arena::DOCUMENT)
        .find(|&id| arena[id].kind == Kind::Element);
    let Some(root) = root else {
        context.warning("DOMDocument::createAttributeNS(): Document Missing Root Element".into());
        return Ok(Value::Bool(false));
    };
    let mut name = match checked_name(
        argument(args, 1).unwrap_or_default(),
        namespace(args, 0),
        true,
    ) {
        Ok(name) => name,
        Err(error) => {
            drop(state);
            return super::mutation::fail_at(
                context,
                &document,
                "DOMDocument::createAttributeNS",
                error,
            );
        }
    };
    use super::attributes::namespaces::{AttributeBinding, bind};
    bind(arena, root, &mut name, AttributeBinding::Node);
    let id = insert_new(arena, Node::new(Kind::Attribute, name, ""));
    drop(state);
    document.at(id).wrap(context)
}

pub(super) fn create_text(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    create(
        context,
        this,
        "DOMDocument::createTextNode",
        Ok(data(Kind::Text, "#text", args)),
    )
}

pub(super) fn create_comment(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    create(
        context,
        this,
        "DOMDocument::createComment",
        Ok(data(Kind::Comment, "#comment", args)),
    )
}

pub(super) fn create_cdata(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    create(
        context,
        this,
        "DOMDocument::createCDATASection",
        Ok(data(Kind::Cdata, "#cdata-section", args)),
    )
}

pub(super) fn create_fragment(
    context: &mut dyn Context,
    this: This<'_>,
    _: &[Value],
) -> NativeResult {
    create(
        context,
        this,
        "DOMDocument::createDocumentFragment",
        Ok(Node::new(
            Kind::Fragment,
            Name::new("#document-fragment"),
            "",
        )),
    )
}

pub(super) fn create_pi(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    create(
        context,
        this,
        "DOMDocument::createProcessingInstruction",
        processing_instruction(args),
    )
}

pub(super) fn create_entity(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    create(
        context,
        this,
        "DOMDocument::createEntityReference",
        entity(args),
    )
}

pub(super) fn construct_element(
    _: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let uri = namespace(args, 2);
    construct(
        this,
        element(
            argument(args, 0),
            argument(args, 1),
            uri.clone(),
            uri.is_some(),
        )?,
    )
}

pub(super) fn construct_attribute(
    _: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let name = checked_name(argument(args, 0).unwrap_or_default(), None, false)?;
    construct(
        this,
        Node::new(Kind::Attribute, name, argument(args, 1).unwrap_or_default()),
    )
}

pub(super) fn construct_text(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    construct(this, data(Kind::Text, "#text", args))
}

pub(super) fn construct_comment(
    _: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    construct(this, data(Kind::Comment, "#comment", args))
}

pub(super) fn construct_cdata(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    construct(this, data(Kind::Cdata, "#cdata-section", args))
}

pub(super) fn construct_fragment(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    construct(
        this,
        Node::new(Kind::Fragment, Name::new("#document-fragment"), ""),
    )
}

pub(super) fn construct_pi(_: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    construct(this, processing_instruction(args)?)
}

pub(super) fn construct_entity(
    _: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    construct(this, entity(args)?)
}

fn data(kind: Kind, name: &str, arguments: &[Value]) -> Node {
    Node::new(
        kind,
        Name::new(name),
        argument(arguments, 0).unwrap_or_default(),
    )
}

fn element(
    name: Option<String>,
    value: Option<String>,
    namespace: Option<String>,
    ns: bool,
) -> Result<Node, NativeError> {
    let name = checked_name(name.unwrap_or_default(), namespace, ns)?;
    Ok(Node::new(Kind::Element, name, value.unwrap_or_default()))
}

fn processing_instruction(arguments: &[Value]) -> Result<Node, NativeError> {
    let target = checked_name(argument(arguments, 0).unwrap_or_default(), None, false)?;
    Ok(Node::new(
        Kind::ProcessingInstruction,
        target,
        argument(arguments, 1).unwrap_or_default(),
    ))
}

fn entity(arguments: &[Value]) -> Result<Node, NativeError> {
    let name = checked_name(argument(arguments, 0).unwrap_or_default(), None, false)?;
    Ok(Node::new(Kind::EntityReference, name, ""))
}

pub(super) fn checked_name(
    name: String,
    namespace: Option<String>,
    ns: bool,
) -> Result<Name, NativeError> {
    if !names::valid(&name) {
        return Err(dom_error(5, "Invalid Character Error"));
    }
    if !ns {
        return Ok(Name::new(name));
    }
    let name = names::qualified(&name, namespace);
    let prefix = name.prefix.as_deref();
    let namespace = name.namespace.as_deref();
    let xmlns_name = prefix == Some("xmlns") || (prefix.is_none() && name.local == "xmlns");
    if !names::valid(&name.local)
        || name.local.contains(':')
        || prefix.is_some_and(|prefix| !names::valid(prefix) || namespace.is_none())
        || (prefix == Some("xml") && namespace != Some(XML))
        || (xmlns_name != (namespace == Some(XMLNS)))
    {
        return Err(dom_error(14, "Namespace Error"));
    }
    Ok(name)
}

fn create(
    context: &mut dyn Context,
    this: This<'_>,
    method: &str,
    node: Result<Node, NativeError>,
) -> NativeResult {
    let location = this_node(this)?;
    let id = match node {
        Ok(node) => insert_new(&mut location.shared.borrow_mut().document.arena, node),
        Err(error) => return super::mutation::fail_at(context, &location, method, error),
    };
    location.at(id).wrap(context)
}

fn construct(this: This<'_>, node: Node) -> NativeResult {
    let mut document = Document::default();
    let id = insert_new(&mut document.arena, node);
    Location::new(document, false, id).bind(receiver(this)?);
    Ok(Value::Null)
}

pub(super) fn insert_new(arena: &mut Arena, mut node: Node) -> NodeId {
    let element = node.kind == Kind::Element;
    let inner = element.then(|| std::mem::take(&mut node.value));
    let declaration = if element {
        node.name
            .namespace
            .as_ref()
            .filter(|uri| uri.as_str() != XML)
            .map(|uri| {
                let name = match &node.name.prefix {
                    Some(prefix) => format!("xmlns:{prefix}"),
                    None => "xmlns".into(),
                };
                Node::new(
                    Kind::Attribute,
                    names::qualified(&name, Some(XMLNS.into())),
                    uri,
                )
            })
    } else {
        None
    };
    let id = match node.kind {
        Kind::Attribute => arena.attribute(node, false),
        _ => arena.allocate(node),
    };
    if let Some(declaration) = declaration {
        let attribute = arena.allocate(declaration);
        let _ = arena.set_attribute(id, attribute, None);
    }
    if let Some(value) = inner.filter(|value| !value.is_empty()) {
        let child = arena.allocate(Node::new(Kind::Text, Name::new("#text"), value));
        let _ = arena.insert(id, child, None);
    }
    id
}
