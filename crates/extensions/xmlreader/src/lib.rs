//! Pull XML events from the shared DOM arena.
//!
//! ```php
//! $reader = XMLReader::fromString('<root><item/></root>');
//! while ($reader->read()) echo $reader->name, PHP_EOL;
//! ```
//!
//! The XML parser never loads external entities from any input.

mod attributes;
mod cursor;
mod factory;
mod markup;
mod properties;
mod read;
mod state;

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, Extension, MethodEntry, NativeClone, Param, ParamType,
    Registry,
};

pub struct XmlReader;

impl Extension for XmlReader {
    fn name(&self) -> &'static str {
        "xmlreader"
    }

    fn register(&self, registry: &mut Registry) {
        registry.class(ClassEntry {
            constants: CONSTANTS,
            properties: properties::PROPERTIES,
            methods: METHODS,
            hooks: Some(&HOOKS),
            ..ClassEntry::class(factory::CLASS)
        });
    }
}

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    forbid_serialization: true,
    property_access: Some(&properties::ACCESS),
    virtual_properties: true,
    properties: Some(properties::project),
    fork: Some(state::fork),
    ..ClassHooks::EMPTY
};

const fn string(name: &'static str) -> Param {
    Param::new(name, ParamType::String)
}

const fn integer(name: &'static str) -> Param {
    Param::new(name, ParamType::Int)
}

const NAME: Param = string("name");
const NS: Param = string("namespace");
const ENCODING: Param = string("encoding")
    .nullable()
    .with_default(ConstantValue::Null);
const FLAGS: Param = integer("flags").with_default(ConstantValue::Int(0));
const URI: &[Param] = &[string("uri"), ENCODING, FLAGS];
const SOURCE: &[Param] = &[string("source"), ENCODING, FLAGS];
const PROPERTY: Param = integer("property");
const FILENAME: Param = string("filename").nullable();
const INDEX: Param = integer("index");

const METHODS: &[MethodEntry] = &[
    MethodEntry::public("close", &[], read::close).tentative_return("true"),
    MethodEntry::public("getAttribute", &[NAME], attributes::get).tentative_return("?string"),
    MethodEntry::public("getAttributeNo", &[INDEX], attributes::get_number)
        .tentative_return("?string"),
    MethodEntry::public("getAttributeNs", &[NAME, NS], attributes::get_ns)
        .tentative_return("?string"),
    MethodEntry::public("getParserProperty", &[PROPERTY], read::get_property)
        .tentative_return("bool"),
    MethodEntry::public("isValid", &[], read::is_valid).tentative_return("bool"),
    MethodEntry::public(
        "lookupNamespace",
        &[string("prefix")],
        attributes::lookup_namespace,
    )
    .tentative_return("?string"),
    MethodEntry::public("moveToAttribute", &[NAME], attributes::move_to).tentative_return("bool"),
    MethodEntry::public("moveToAttributeNo", &[INDEX], attributes::move_to_number)
        .tentative_return("bool"),
    MethodEntry::public("moveToAttributeNs", &[NAME, NS], attributes::move_ns)
        .tentative_return("bool"),
    MethodEntry::public("moveToElement", &[], attributes::to_element).tentative_return("bool"),
    MethodEntry::public("moveToFirstAttribute", &[], attributes::first).tentative_return("bool"),
    MethodEntry::public("moveToNextAttribute", &[], attributes::next).tentative_return("bool"),
    MethodEntry::public_async("read", &[], read::read).tentative_return("bool"),
    MethodEntry::public_async(
        "next",
        &[NAME.nullable().with_default(ConstantValue::Null)],
        read::next,
    )
    .tentative_return("bool"),
    MethodEntry::public_async("open", URI, factory::open)
        .static_()
        .with_instance_receiver(),
    MethodEntry::public_async("fromUri", URI, factory::from_uri)
        .static_()
        .returning("static"),
    MethodEntry::public_async(
        "fromStream",
        &[
            Param::new("stream", ParamType::Mixed),
            ENCODING,
            FLAGS,
            string("documentUri")
                .nullable()
                .with_default(ConstantValue::Null),
        ],
        factory::from_stream,
    )
    .static_()
    .returning("static"),
    MethodEntry::public("readInnerXml", &[], markup::inner_xml).tentative_return("string"),
    MethodEntry::public("readOuterXml", &[], markup::outer_xml).tentative_return("string"),
    MethodEntry::public("readString", &[], markup::string).tentative_return("string"),
    MethodEntry::public("setSchema", &[FILENAME], markup::unsupported_schema)
        .tentative_return("bool"),
    MethodEntry::public(
        "setParserProperty",
        &[PROPERTY, Param::new("value", ParamType::Bool)],
        read::set_property,
    )
    .tentative_return("bool"),
    MethodEntry::public("setRelaxNGSchema", &[FILENAME], markup::unsupported_schema)
        .tentative_return("bool"),
    MethodEntry::public(
        "setRelaxNGSchemaSource",
        &[string("source").nullable()],
        markup::unsupported_schema,
    )
    .tentative_return("bool"),
    MethodEntry::public_async("XML", SOURCE, factory::xml)
        .static_()
        .with_instance_receiver(),
    MethodEntry::public_async("fromString", SOURCE, factory::from_string)
        .static_()
        .returning("static"),
    MethodEntry::public(
        "expand",
        &[Param::new("baseNode", ParamType::Class("DOMNode"))
            .nullable()
            .with_default(ConstantValue::Null)],
        markup::expand,
    )
    .tentative_return("DOMNode|false"),
];

const CONSTANTS: &[(&str, ConstantValue)] = &[
    ("NONE", ConstantValue::Int(0)),
    ("ELEMENT", ConstantValue::Int(1)),
    ("ATTRIBUTE", ConstantValue::Int(2)),
    ("TEXT", ConstantValue::Int(3)),
    ("CDATA", ConstantValue::Int(4)),
    ("ENTITY_REF", ConstantValue::Int(5)),
    ("ENTITY", ConstantValue::Int(6)),
    ("PI", ConstantValue::Int(7)),
    ("COMMENT", ConstantValue::Int(8)),
    ("DOC", ConstantValue::Int(9)),
    ("DOC_TYPE", ConstantValue::Int(10)),
    ("DOC_FRAGMENT", ConstantValue::Int(11)),
    ("NOTATION", ConstantValue::Int(12)),
    ("WHITESPACE", ConstantValue::Int(13)),
    ("SIGNIFICANT_WHITESPACE", ConstantValue::Int(14)),
    ("END_ELEMENT", ConstantValue::Int(15)),
    ("END_ENTITY", ConstantValue::Int(16)),
    ("XML_DECLARATION", ConstantValue::Int(17)),
    ("LOADDTD", ConstantValue::Int(1)),
    ("DEFAULTATTRS", ConstantValue::Int(2)),
    ("VALIDATE", ConstantValue::Int(3)),
    ("SUBST_ENTITIES", ConstantValue::Int(4)),
];
