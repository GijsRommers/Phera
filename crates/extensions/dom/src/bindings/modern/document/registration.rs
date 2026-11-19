//! Register the modern document factories and the shared document methods.
//!
//! ```php
//! $html = Dom\HTMLDocument::createFromFile('index.html', LIBXML_NOERROR);
//! ```

use phera_extension_api::{
    ClassEntry, ClassKind, ConstantValue as C, MethodEntry as M, Param as P, ParamType as T,
    Registry,
};

use crate::tree::Kind;

use super::super::super::{collection, document, factory, mutation};
use super::super::selectors;
use super::{Format, create_empty, create_node, from_string, new_html_file, new_xml_file};

const NODE: T = T::Class("Dom\\Node");
const SOURCE: P = P::new("source", T::String);
const PATH: P = P::new("path", T::String);
const OPTIONS: P = P::new("options", T::Int).with_default(C::Int(0));
const ENCODING: P = P::new("encoding", T::String).with_default(C::String("UTF-8"));
const OVERRIDE: P = P::new("overrideEncoding", T::String)
    .nullable()
    .with_default(C::Null);
const SAVE_NODE: P = P::new("node", NODE).nullable().with_default(C::Null);
const FILENAME: P = P::new("filename", T::String);
const NAME: P = P::new("localName", T::String);
const QUALIFIED: P = P::new("qualifiedName", T::String);
const NS: P = P::new("namespace", T::String).nullable();
const DATA: P = P::new("data", T::String);
const DEEP: P = P::new("deep", T::Bool).with_default(C::Bool(false));
const SELECTORS: P = P::new("selectors", T::String);
const VARIADIC: P = document::VARIADIC;

pub(in crate::bindings::modern) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("Dom\\Node"),
        kind: ClassKind::AbstractClass,
        methods: const {
            &[
                M::public("createElement", &[NAME], |c, t, a| {
                    create_node(c, t, a, Kind::Element, false)
                })
                .returning("Dom\\Element"),
                M::public("createElementNS", &[NS, QUALIFIED], |c, t, a| {
                    create_node(c, t, a, Kind::Element, true)
                })
                .returning("Dom\\Element"),
                M::public("createAttribute", &[NAME], |c, t, a| {
                    create_node(c, t, a, Kind::Attribute, false)
                })
                .returning("Dom\\Attr"),
                M::public("createAttributeNS", &[NS, QUALIFIED], |c, t, a| {
                    create_node(c, t, a, Kind::Attribute, true)
                })
                .returning("Dom\\Attr"),
                M::public("createDocumentFragment", &[], factory::create_fragment)
                    .returning("Dom\\DocumentFragment"),
                M::public("createTextNode", &[DATA], factory::create_text).returning("Dom\\Text"),
                M::public("createComment", &[DATA], factory::create_comment)
                    .returning("Dom\\Comment"),
                M::public("createCDATASection", &[DATA], factory::create_cdata)
                    .returning("Dom\\CDATASection"),
                M::public(
                    "createProcessingInstruction",
                    const { &[P::new("target", T::String), DATA] },
                    factory::create_pi,
                )
                .returning("Dom\\ProcessingInstruction"),
                M::public(
                    "getElementById",
                    const { &[P::new("elementId", T::String)] },
                    document::by_id,
                )
                .returning("?Dom\\Element"),
                M::public("getElementsByTagName", &[QUALIFIED], collection::tags)
                    .returning("Dom\\HTMLCollection"),
                M::public("getElementsByTagNameNS", &[NS, NAME], collection::tags_ns)
                    .returning("Dom\\HTMLCollection"),
                M::public(
                    "importNode",
                    const { &[P::new("node", NODE).nullable(), DEEP] },
                    mutation::import,
                )
                .returning("Dom\\Node"),
                M::public(
                    "importLegacyNode",
                    const { &[P::new("node", T::Class("DOMNode")), DEEP] },
                    mutation::import,
                )
                .returning("Dom\\Node"),
                M::public(
                    "adoptNode",
                    const { &[P::new("node", NODE)] },
                    mutation::adopt,
                )
                .returning("Dom\\Node"),
                M::public("append", &[VARIADIC], mutation::append).returning("void"),
                M::public("prepend", &[VARIADIC], mutation::prepend).returning("void"),
                M::public("replaceChildren", &[VARIADIC], mutation::replace_children)
                    .returning("void"),
                M::public("querySelector", &[SELECTORS], selectors::first)
                    .returning("?Dom\\Element"),
                M::public("querySelectorAll", &[SELECTORS], selectors::all)
                    .returning("Dom\\NodeList"),
            ]
        },
        ..ClassEntry::class("Dom\\Document")
    });
    registry.class(ClassEntry {
        parent: Some("Dom\\Document"),
        kind: ClassKind::FinalClass,
        methods: const {
            &[
                M::public("createEmpty", &[ENCODING], |c, _, a| {
                    create_empty(c, a, Format::Html)
                })
                .static_()
                .returning("Dom\\HTMLDocument"),
                M::public(
                    "createFromString",
                    &[SOURCE, OPTIONS, OVERRIDE],
                    |c, _, a| from_string(c, a, Format::Html),
                )
                .static_()
                .returning("Dom\\HTMLDocument"),
                M::public_async("createFromFile", &[PATH, OPTIONS, OVERRIDE], new_html_file)
                    .static_()
                    .returning("Dom\\HTMLDocument"),
                M::public("saveXml", &[SAVE_NODE, OPTIONS], document::io::save_xml)
                    .returning("string|false"),
                M::public("saveHtml", &[SAVE_NODE], document::io::save_html).returning("string"),
                M::public_async("saveXmlFile", &[FILENAME, OPTIONS], document::io::save_file)
                    .returning("int|false"),
                M::public_async("saveHtmlFile", &[FILENAME], document::io::save_html_file)
                    .returning("int|false"),
            ]
        },
        ..ClassEntry::class("Dom\\HTMLDocument")
    });
    registry.class(ClassEntry {
        parent: Some("Dom\\Document"),
        kind: ClassKind::FinalClass,
        methods: const {
            &[
                M::public(
                    "createEmpty",
                    const {
                        &[
                            P::new("version", T::String).with_default(C::String("1.0")),
                            ENCODING,
                        ]
                    },
                    |c, _, a| create_empty(c, a, Format::Xml),
                )
                .static_()
                .returning("Dom\\XMLDocument"),
                M::public(
                    "createFromString",
                    &[SOURCE, OPTIONS, OVERRIDE],
                    |c, _, a| from_string(c, a, Format::Xml),
                )
                .static_()
                .returning("Dom\\XMLDocument"),
                M::public_async("createFromFile", &[PATH, OPTIONS, OVERRIDE], new_xml_file)
                    .static_()
                    .returning("Dom\\XMLDocument"),
                M::public("saveXml", &[SAVE_NODE, OPTIONS], document::io::save_xml)
                    .returning("string|false"),
                M::public_async("saveXmlFile", &[FILENAME, OPTIONS], document::io::save_file)
                    .returning("int|false"),
                M::public(
                    "createEntityReference",
                    const { &[P::new("name", T::String)] },
                    factory::create_entity,
                )
                .returning("Dom\\EntityReference"),
            ]
        },
        ..ClassEntry::class("Dom\\XMLDocument")
    });
}
