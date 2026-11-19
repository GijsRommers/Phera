//! Construct detached nodes and documents with PHP's DOM factories.
//!
//! ```php
//! $node = $document->createElementNS('urn:foo', 'f:item');
//! $document->appendChild($node);
//! ```

use phera_extension_api::{
    ClassEntry, ConstantValue as C, Context, MethodEntry as M, NativeError, NativeResult,
    Param as P, ParamType as T, Registry, Value, args,
};

use crate::document::Document;
use crate::tree::{Arena, Doctype, Kind, Name, Node, NodeId};
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
        name.map(|name| Node::new(Kind::Attribute, name, "")),
    )
}

pub(super) fn create_attribute_ns(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    let name = checked_name(
        argument(args, 1).unwrap_or_default(),
        namespace(args, 0),
        true,
    );
    create(
        context,
        this,
        name.map(|name| Node::new(Kind::Attribute, name, "")),
    )
}

pub(super) fn create_text(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    create(context, this, Ok(data(Kind::Text, "#text", args)))
}

pub(super) fn create_comment(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    create(context, this, Ok(data(Kind::Comment, "#comment", args)))
}

pub(super) fn create_cdata(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    create(context, this, Ok(data(Kind::Cdata, "#cdata-section", args)))
}

pub(super) fn create_fragment(
    context: &mut dyn Context,
    this: This<'_>,
    _: &[Value],
) -> NativeResult {
    create(
        context,
        this,
        Ok(Node::new(
            Kind::Fragment,
            Name::new("#document-fragment"),
            "",
        )),
    )
}

pub(super) fn create_pi(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    create(context, this, processing_instruction(args))
}

pub(super) fn create_entity(
    context: &mut dyn Context,
    this: This<'_>,
    args: &[Value],
) -> NativeResult {
    create(context, this, entity(args))
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
    node: Result<Node, NativeError>,
) -> NativeResult {
    let location = this_node(this)?;
    let id = match node {
        Ok(node) => insert_new(&mut location.shared.borrow_mut().document.arena, node),
        Err(error) => return super::mutation::fail(context, &location, error),
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
    let id = arena.allocate(node);
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

pub(super) fn has_feature(_: &mut dyn Context, _: This<'_>, args: &[Value]) -> NativeResult {
    let feature = argument(args, 0).unwrap_or_default().to_ascii_lowercase();
    let version = argument(args, 1).unwrap_or_default();
    Ok(Value::Bool(
        matches!(feature.as_str(), "xml" | "core")
            && matches!(version.as_str(), "" | "1.0" | "2.0"),
    ))
}

fn create_doctype(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let name = argument(arguments, 0).unwrap_or_default();
    let name = checked_name(name, None, false)?;
    let mut document = Document::default();
    let mut node = Node::new(Kind::Doctype, name, "");
    node.doctype = Some(Doctype {
        public_id: argument(arguments, 1).unwrap_or_default(),
        system_id: argument(arguments, 2).unwrap_or_default(),
        internal_subset: None,
    });
    let id = document.arena.allocate(node);
    Location::new(document, false, id).wrap(context)
}

fn create_document(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let mut document = Document::default();
    let doctype = args::object(arguments, 2)
        .map(|object| location(object))
        .transpose()?;
    if let Some(doctype) = &doctype {
        let source = doctype.shared.borrow();
        if source.owned {
            return Err(dom_error(4, "Wrong Document Error"));
        }
        let copy = document
            .arena
            .import(&source.document.arena, doctype.id, true);
        let _ = document.arena.insert(Arena::DOCUMENT, copy, None);
    }
    let name = argument(arguments, 1).unwrap_or_default();
    if !name.is_empty() {
        let root = element(Some(name), None, namespace(arguments, 0), true)?;
        let id = insert_new(&mut document.arena, root);
        let _ = document.arena.insert(Arena::DOCUMENT, id, None);
    }
    let document = Location::new(document, true, Arena::DOCUMENT);
    if let Some(object) = args::object(arguments, 2) {
        let id = document.shared.borrow().document.arena[Arena::DOCUMENT].first_child();
        if let Some(id) = id {
            document.at(id).bind(object);
        }
    }
    document.wrap(context)
}

pub(super) fn register(registry: &mut Registry) {
    const EMPTY: C = C::String("");
    registry.class(ClassEntry {
        methods: const {
            const {
                &[
                    M::public(
                        "hasFeature",
                        const { &[P::new("feature", T::String), P::new("version", T::String)] },
                        has_feature,
                    )
                    .tentative_return("bool"),
                    M::public(
                        "createDocumentType",
                        const {
                            &[
                                P::new("qualifiedName", T::String),
                                P::new("publicId", T::String).with_default(EMPTY),
                                P::new("systemId", T::String).with_default(EMPTY),
                            ]
                        },
                        create_doctype,
                    ),
                    M::public(
                        "createDocument",
                        const {
                            &[
                                P::new("namespace", T::String)
                                    .nullable()
                                    .with_default(C::Null),
                                P::new("qualifiedName", T::String).with_default(EMPTY),
                                P::new("doctype", T::Class("DOMDocumentType"))
                                    .nullable()
                                    .with_default(C::Null),
                            ]
                        },
                        create_document,
                    )
                    .tentative_return("DOMDocument"),
                ]
            }
        },
        ..ClassEntry::class("DOMImplementation")
    });
}
