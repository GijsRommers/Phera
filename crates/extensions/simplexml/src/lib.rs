//! SimpleXML views with live DOM interoperability.
//!
//! ```php
//! $xml = simplexml_load_string('<article id="1"><title>Hello</title></article>');
//! echo $xml['id'], ': ', $xml->title;
//! ```
//!
//! Document strings use DOM's bounded entity processing and never load an
//! external entity.

mod dimensions;
mod iteration;
mod load;
mod methods;
mod namespaces;
mod operators;
mod projection;
mod properties;
mod selection;

use phera_extension_api::{
    ClassEntry, ClassHooks, ConstantValue, Extension, FunctionEntry, MethodEntry, NativeClone,
    Param, ParamType, Registry,
};
use selection::CLASS;

pub struct SimpleXml;

impl Extension for SimpleXml {
    fn name(&self) -> &'static str {
        "simplexml"
    }

    fn php_module(&self) -> Option<&'static str> {
        Some("SimpleXML")
    }

    fn register(&self, registry: &mut Registry) {
        registry.class(ClassEntry {
            methods: METHODS,
            hooks: Some(&HOOKS),
            interfaces: &["Stringable", "Countable", "RecursiveIterator"],
            ..ClassEntry::class(CLASS)
        });
        registry.class(ClassEntry {
            parent: Some(CLASS),
            ..ClassEntry::class("SimpleXMLIterator")
        });
        for (entry, result, module) in FUNCTIONS {
            registry.function(*entry);
            registry.describe_function(entry.name, result, *module);
        }
    }
}

static HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Copy(projection::clone)),
    cast: Some(operators::cast),
    compare: Some(operators::compare),
    binary: Some(operators::binary),
    unary: Some(operators::unary),
    properties: Some(projection::properties),
    property_access: Some(&properties::ACCESS),
    iterator: Some(iteration::iterator),
    iterator_by_reference: Some(iteration::refuse_by_reference),
    dimensions: Some(dimensions::ACCESS),
    ..ClassHooks::EMPTY
};

const fn string(name: &'static str) -> Param {
    Param::new(name, ParamType::String)
}

const fn boolean(name: &'static str, default: bool) -> Param {
    Param::new(name, ParamType::Bool).with_default(ConstantValue::Bool(default))
}

const FLAGS: Param = Param::new("options", ParamType::Int).with_default(ConstantValue::Int(0));
const FN_CLASS: Param = string("class_name")
    .nullable()
    .with_constant_default("SimpleXMLElement::class", ConstantValue::String(CLASS));
const FN_PREFIX: Param = boolean("is_prefix", false);
const FN_NAMESPACE: Param = string("namespace_or_prefix").with_default(ConstantValue::String(""));
const PREFIX: Param = boolean("isPrefix", false);
const NAMESPACE: Param = string("namespace")
    .nullable()
    .with_default(ConstantValue::Null);
const NAMESPACE_OR_PREFIX: Param =
    string("namespaceOrPrefix").with_default(ConstantValue::String(""));
const FILENAME: Param = string("filename")
    .nullable()
    .with_default(ConstantValue::Null);
const QUALIFIED_NAME: Param = string("qualifiedName");
const VALUE: Param = string("value");
const RECURSIVE: Param = boolean("recursive", false);
const OBJECT: Param = Param::new("node", ParamType::Object);

const FUNCTIONS: &[(FunctionEntry, &str, Option<&str>)] = &[
    (
        FunctionEntry::asynchronous(
            "simplexml_load_file",
            &[string("filename"), FN_CLASS, FLAGS, FN_NAMESPACE, FN_PREFIX],
            load::load_file,
        ),
        "SimpleXMLElement|false",
        None,
    ),
    (
        FunctionEntry::asynchronous(
            "simplexml_load_string",
            &[string("data"), FN_CLASS, FLAGS, FN_NAMESPACE, FN_PREFIX],
            load::load_string,
        ),
        "SimpleXMLElement|false",
        None,
    ),
    (
        FunctionEntry::asynchronous(
            "simplexml_import_dom",
            &[OBJECT, FN_CLASS],
            load::import_dom,
        ),
        "?SimpleXMLElement",
        None,
    ),
    (
        FunctionEntry::sync("dom_import_simplexml", &[OBJECT], load::import_simplexml),
        "DOMElement|DOMAttr",
        Some("dom"),
    ),
];

const METHODS: &[MethodEntry] = &[
    MethodEntry::public("xpath", &[string("expression")], namespaces::query)
        .tentative_return("array|null|false"),
    MethodEntry::public(
        "registerXPathNamespace",
        &[string("prefix"), string("namespace")],
        namespaces::register_xpath,
    )
    .tentative_return("bool"),
    MethodEntry::public_async("asXML", &[FILENAME], methods::save).tentative_return("string|bool"),
    MethodEntry::public_async("saveXML", &[FILENAME], methods::save)
        .tentative_return("string|bool"),
    MethodEntry::public("getNamespaces", &[RECURSIVE], namespaces::get_namespaces)
        .tentative_return("array"),
    MethodEntry::public(
        "getDocNamespaces",
        &[RECURSIVE, boolean("fromRoot", true)],
        namespaces::get_doc_namespaces,
    )
    .tentative_return("array|false"),
    MethodEntry::public(
        "children",
        &[
            NAMESPACE_OR_PREFIX
                .nullable()
                .with_default(ConstantValue::Null),
            PREFIX,
        ],
        methods::children,
    )
    .tentative_return("?SimpleXMLElement"),
    MethodEntry::public(
        "attributes",
        &[
            NAMESPACE_OR_PREFIX
                .nullable()
                .with_default(ConstantValue::Null),
            PREFIX,
        ],
        methods::attributes,
    )
    .tentative_return("?SimpleXMLElement"),
    MethodEntry::public_async(
        "__construct",
        &[
            string("data"),
            FLAGS,
            boolean("dataIsURL", false),
            NAMESPACE_OR_PREFIX,
            PREFIX,
        ],
        load::construct,
    ),
    MethodEntry::public(
        "addChild",
        &[
            QUALIFIED_NAME,
            VALUE.nullable().with_default(ConstantValue::Null),
            NAMESPACE,
        ],
        methods::add_child,
    )
    .tentative_return("?SimpleXMLElement"),
    MethodEntry::public(
        "addAttribute",
        &[QUALIFIED_NAME, VALUE, NAMESPACE],
        methods::add_attribute,
    )
    .tentative_return("void"),
    MethodEntry::public("getName", &[], methods::name).tentative_return("string"),
    MethodEntry::public("__toString", &[], methods::stringify).returning("string"),
    MethodEntry::public("__debugInfo", &[], projection::debug).returning("?array"),
    MethodEntry::public("count", &[], methods::count).tentative_return("int"),
    MethodEntry::public("rewind", &[], iteration::rewind).tentative_return("void"),
    MethodEntry::public("valid", &[], iteration::valid).tentative_return("bool"),
    MethodEntry::public("current", &[], iteration::current).tentative_return("SimpleXMLElement"),
    MethodEntry::public("key", &[], iteration::key).tentative_return("string"),
    MethodEntry::public("next", &[], iteration::advance).tentative_return("void"),
    MethodEntry::public("hasChildren", &[], iteration::has_children).tentative_return("bool"),
    MethodEntry::public("getChildren", &[], iteration::get_children)
        .tentative_return("?SimpleXMLElement"),
];
