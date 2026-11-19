//! DOMImplementation document and doctype factories.
//!
//! ```php
//! $document = (new DOMImplementation)->createDocument(null, 'root');
//! ```

use super::{Location, This, argument, dom_error, element, insert_new, location, namespace};
use crate::document::Document;
use crate::tree::{Arena, Doctype, Kind, Name, Node};
use phera_extension_api::{
    ClassEntry, ConstantValue as C, Context, MethodEntry as M, NativeError, NativeResult,
    Param as P, ParamType as T, Registry, Value, args,
};

pub(in crate::bindings) fn has_feature(
    _: &mut dyn Context,
    _: This<'_>,
    args: &[Value],
) -> NativeResult {
    let feature = argument(args, 0).unwrap_or_default().to_ascii_lowercase();
    let version = argument(args, 1).unwrap_or_default();
    Ok(Value::Bool(
        matches!(feature.as_str(), "xml" | "core")
            && matches!(version.as_str(), "" | "1.0" | "2.0"),
    ))
}

fn create_doctype(context: &mut dyn Context, _: This<'_>, arguments: &[Value]) -> NativeResult {
    let name = argument(arguments, 0).unwrap_or_default();
    const METHOD: &str = "DOMImplementation::createDocumentType()";
    if name.is_empty() {
        return Err(NativeError::value_error(format!(
            "{METHOD}: Argument #1 ($qualifiedName) must not be empty",
        )));
    }
    let name = name.split('\0').next().unwrap_or_default();
    if name.contains("%00") {
        context.warning(format!(
            "{METHOD}: URI must not contain percent-encoded NUL bytes"
        ));
        return Ok(Value::Bool(false));
    }
    let mut document = Document::default();
    let mut node = Node::new(Kind::Doctype, Name::new(name), "");
    node.doctype = Some(Box::new(Doctype {
        public_id: argument(arguments, 1).unwrap_or_default(),
        system_id: argument(arguments, 2).unwrap_or_default(),
        ..Doctype::default()
    }));
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

pub(in crate::bindings) fn register(registry: &mut Registry) {
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
