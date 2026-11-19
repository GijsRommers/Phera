//! PHP's legacy document methods and their argument signatures.
//!
//! ```php
//! $document = new DOMDocument();
//! $document->loadXML('<r><child/></r>');
//! echo $document->saveXML();
//! ```

mod html_file;
pub(super) mod io;

use phera_extension_api::{
    ClassEntry, ConstantValue as C, Context, MethodEntry as M, NativeError, NativeResult,
    Param as P, ParamType as T, Registry, Value, args,
};

use crate::document::{Document, Settings};
use crate::tree::{Arena, Kind};

use super::{Location, This, collection, factory, mutation, node, receiver, this_node};

const NODE: T = T::Class("DOMNode");
const OPTIONS: P = P::new("options", T::Int).with_default(C::Int(0));
const FILE: P = P::new("filename", T::String);
const SOURCE: P = P::new("source", T::String);
const SAVE_NODE: P = P::new("node", NODE).nullable().with_default(C::Null);
const NS: P = P::new("namespace", T::String).nullable();
const QUALIFIED: P = P::new("qualifiedName", T::String);
const LOCAL: P = P::new("localName", T::String);
const TEXT: P = P::new("data", T::String);
const CONTENT: P = P::new("value", T::String).with_default(C::String(""));
const DEEP: P = P::new("deep", T::Bool).with_default(C::Bool(false));
pub(super) const VARIADIC: P = P::new("nodes", T::Mixed).variadic();

pub(super) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        parent: Some("DOMNode"),
        interfaces: &["DOMParentNode"],
        methods: const {
            const {
                &[
                    M::public(
                        "__construct",
                        const {
                            &[
                                P::new("version", T::String).with_default(C::String("1.0")),
                                P::new("encoding", T::String).with_default(C::String("")),
                            ]
                        },
                        construct,
                    ),
                    M::public("loadXML", &[SOURCE, OPTIONS], io::load_xml).tentative_return("bool"),
                    M::public("loadHTML", &[SOURCE, OPTIONS], io::load_html)
                        .tentative_return("bool"),
                    M::public_async("load", &[FILE, OPTIONS], io::load_file)
                        .tentative_return("bool"),
                    M::public_async("loadHTMLFile", &[FILE, OPTIONS], io::load_html_file)
                        .tentative_return("bool"),
                    M::public("saveXML", &[SAVE_NODE, OPTIONS], io::save_xml)
                        .tentative_return("string|false"),
                    M::public("saveHTML", &[SAVE_NODE], io::save_html)
                        .tentative_return("string|false"),
                    M::public_async("save", &[FILE, OPTIONS], io::save_file)
                        .tentative_return("int|false"),
                    M::public_async("saveHTMLFile", &[FILE], io::save_html_file)
                        .tentative_return("int|false"),
                    M::public("createElement", &[LOCAL, CONTENT], factory::create_element),
                    M::public(
                        "createElementNS",
                        &[NS, QUALIFIED, CONTENT],
                        factory::create_element_ns,
                    ),
                    M::public("createAttribute", &[LOCAL], factory::create_attribute),
                    M::public(
                        "createAttributeNS",
                        &[NS, QUALIFIED],
                        factory::create_attribute_ns,
                    ),
                    M::public("createTextNode", &[TEXT], factory::create_text)
                        .tentative_return("DOMText"),
                    M::public("createComment", &[TEXT], factory::create_comment)
                        .tentative_return("DOMComment"),
                    M::public("createCDATASection", &[TEXT], factory::create_cdata),
                    M::public("createDocumentFragment", &[], factory::create_fragment)
                        .tentative_return("DOMDocumentFragment"),
                    M::public(
                        "createProcessingInstruction",
                        const {
                            &[
                                P::new("target", T::String),
                                TEXT.with_default(C::String("")),
                            ]
                        },
                        factory::create_pi,
                    ),
                    M::public(
                        "createEntityReference",
                        const { &[P::new("name", T::String)] },
                        factory::create_entity,
                    ),
                    M::public(
                        "importNode",
                        const { &[P::new("node", NODE), DEEP] },
                        mutation::import,
                    ),
                    M::public(
                        "adoptNode",
                        const { &[P::new("node", NODE)] },
                        mutation::adopt,
                    )
                    .returning("DOMNode|false"),
                    M::public(
                        "getElementById",
                        const { &[P::new("elementId", T::String)] },
                        by_id,
                    )
                    .tentative_return("?DOMElement"),
                    M::public("getElementsByTagName", &[QUALIFIED], collection::tags)
                        .tentative_return("DOMNodeList"),
                    M::public("getElementsByTagNameNS", &[NS, LOCAL], collection::tags_ns)
                        .tentative_return("DOMNodeList"),
                    M::public("normalizeDocument", &[], node::normalize).tentative_return("void"),
                    M::public("append", &[VARIADIC], mutation::append).returning("void"),
                    M::public("prepend", &[VARIADIC], mutation::prepend).returning("void"),
                    M::public("replaceChildren", &[VARIADIC], mutation::replace_children)
                        .returning("void"),
                    M::public("validate", &[], |_, _, _| not_supported("validate"))
                        .tentative_return("bool"),
                    M::public(
                        "schemaValidate",
                        const { &[FILE, P::new("flags", T::Int).with_default(C::Int(0))] },
                        |_, _, _| not_supported("schemaValidate"),
                    )
                    .tentative_return("bool"),
                    M::public(
                        "schemaValidateSource",
                        const { &[SOURCE, P::new("flags", T::Int).with_default(C::Int(0))] },
                        |_, _, _| not_supported("schemaValidateSource"),
                    )
                    .tentative_return("bool"),
                    M::public("relaxNGValidate", &[FILE], |_, _, _| {
                        not_supported("relaxNGValidate")
                    })
                    .tentative_return("bool"),
                    M::public("relaxNGValidateSource", &[SOURCE], |_, _, _| {
                        not_supported("relaxNGValidateSource")
                    })
                    .tentative_return("bool"),
                ]
            }
        },
        ..ClassEntry::class("DOMDocument")
    });
}

fn construct(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let settings = Settings {
        version: Some(super::factory::argument(arguments, 0).unwrap_or_else(|| "1.0".into())),
        encoding: super::factory::argument(arguments, 1).filter(|encoding| !encoding.is_empty()),
        ..Settings::default()
    };
    let document = Document {
        settings,
        ..Document::default()
    };
    Location::new(document, true, Arena::DOCUMENT).bind(receiver(this)?);
    Ok(Value::Null)
}

fn by_id(context: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let location = this_node(this)?;
    let id = String::from_utf8_lossy(args::bytes(arguments, 0));
    let found = {
        let state = location.shared.borrow();
        let arena = &state.document.arena;
        arena.descendants(location.id).find(|&node| {
            arena[node].kind == Kind::Element
                && arena[node]
                    .attributes()
                    .iter()
                    .any(|&attr| arena[attr].is_id && arena[attr].value == id)
        })
    };
    location.wrap_optional(context, found)
}

fn not_supported(method: &str) -> NativeResult {
    Err(NativeError::throw(
        "Phera\\NotSupportedException",
        format!("DOMDocument::{method}() is not supported by Phera"),
    ))
}
