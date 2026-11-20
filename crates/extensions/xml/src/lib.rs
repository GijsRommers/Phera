//! PHP's XML event parser and its callbacks.
//!
//! ```php
//! $parser = xml_parser_create();
//! xml_parse_into_struct($parser, '<r><item>value</item></r>', $rows, $index);
//! ```
//!
//! The parser handles UTF-8, ISO-8859-1 and US-ASCII. Entity declarations use
//! the same bounded resolver as DOM. Parsing never reads external entities.

mod bindings;
mod callbacks;
mod errors;
mod events;
mod options;
mod structure;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, ConstantValue, Extension, FunctionEntry, MethodEntry,
    NativeClone, Param, ParamType, Registry,
};

use events::Handler;

pub struct Xml;

impl Extension for Xml {
    fn name(&self) -> &'static str {
        "xml"
    }

    fn register(&self, registry: &mut Registry) {
        registry.class(ClassEntry {
            kind: ClassKind::FinalClass,
            hooks: Some(&HOOKS),
            methods: const { &[MethodEntry::public("__construct", &[], bindings::construct)] },
            ..ClassEntry::class(bindings::CLASS)
        });
        for (name, value) in CONSTANTS {
            registry.constant(name, *value);
        }
        registry.constant("XML_SAX_IMPL", "libxml");
        for (function, returned) in FUNCTIONS {
            registry.function(*function);
            registry.describe_function(function.name, returned, None);
        }
    }
}

const HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::Forbidden),
    forbid_serialization: true,
    ..ClassHooks::EMPTY
};

const PARSER: Param = Param::new("parser", ParamType::Class(bindings::CLASS));
const PARSE_OPTIONS: Param = Param::new("option", ParamType::Int);
const VALUE: Param = Param::new("value", ParamType::Mixed);
const ENCODING: Param = Param::new("encoding", ParamType::String)
    .nullable()
    .with_default(ConstantValue::Null);
const DATA: Param = Param::new("data", ParamType::String);
const HANDLER: &[Param] = &[PARSER, Param::new("handler", ParamType::Mixed)];

const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync("xml_parser_create", &[ENCODING], bindings::create),
        "XMLParser",
    ),
    (
        FunctionEntry::sync(
            "xml_parser_create_ns",
            &[
                ENCODING,
                Param::new("separator", ParamType::String).with_default(ConstantValue::String(":")),
            ],
            bindings::create_ns,
        ),
        "XMLParser",
    ),
    (
        FunctionEntry::sync(
            "xml_set_element_handler",
            &[
                PARSER,
                Param::new("start_handler", ParamType::Mixed),
                Param::new("end_handler", ParamType::Mixed),
            ],
            callbacks::elements,
        ),
        "true",
    ),
    (
        FunctionEntry::sync(
            "xml_set_character_data_handler",
            HANDLER,
            |context, arguments| {
                callbacks::register(
                    context,
                    arguments,
                    Handler::Text,
                    "xml_set_character_data_handler",
                )
            },
        ),
        "true",
    ),
    (
        FunctionEntry::sync(
            "xml_set_processing_instruction_handler",
            HANDLER,
            |context, arguments| {
                callbacks::register(
                    context,
                    arguments,
                    Handler::Pi,
                    "xml_set_processing_instruction_handler",
                )
            },
        ),
        "true",
    ),
    (
        FunctionEntry::sync("xml_set_default_handler", HANDLER, |context, arguments| {
            callbacks::register(
                context,
                arguments,
                Handler::Default,
                "xml_set_default_handler",
            )
        }),
        "true",
    ),
    (
        FunctionEntry::sync(
            "xml_set_unparsed_entity_decl_handler",
            HANDLER,
            |context, arguments| {
                callbacks::register(
                    context,
                    arguments,
                    Handler::Unparsed,
                    "xml_set_unparsed_entity_decl_handler",
                )
            },
        ),
        "true",
    ),
    (
        FunctionEntry::sync(
            "xml_set_notation_decl_handler",
            HANDLER,
            |context, arguments| {
                callbacks::register(
                    context,
                    arguments,
                    Handler::Notation,
                    "xml_set_notation_decl_handler",
                )
            },
        ),
        "true",
    ),
    (
        FunctionEntry::sync(
            "xml_set_external_entity_ref_handler",
            HANDLER,
            |context, arguments| {
                callbacks::register(
                    context,
                    arguments,
                    Handler::External,
                    "xml_set_external_entity_ref_handler",
                )
            },
        ),
        "true",
    ),
    (
        FunctionEntry::sync(
            "xml_set_start_namespace_decl_handler",
            HANDLER,
            |context, arguments| {
                callbacks::register(
                    context,
                    arguments,
                    Handler::Namespace,
                    "xml_set_start_namespace_decl_handler",
                )
            },
        ),
        "true",
    ),
    (
        FunctionEntry::sync(
            "xml_set_end_namespace_decl_handler",
            HANDLER,
            |context, arguments| {
                callbacks::register(
                    context,
                    arguments,
                    Handler::NamespaceEnd,
                    "xml_set_end_namespace_decl_handler",
                )
            },
        ),
        "true",
    ),
    (
        FunctionEntry::asynchronous(
            "xml_parse",
            &[
                PARSER,
                DATA,
                Param::new("is_final", ParamType::Bool).with_default(ConstantValue::Bool(false)),
            ],
            bindings::parse,
        ),
        "int",
    ),
    (
        FunctionEntry::asynchronous(
            "xml_parse_into_struct",
            &[
                PARSER,
                DATA,
                Param::new("values", ParamType::Mixed).by_ref(),
                Param::new("index", ParamType::Mixed)
                    .by_ref()
                    .with_default(ConstantValue::Null),
            ],
            bindings::parse_into_struct,
        ),
        "int|false",
    ),
    (
        FunctionEntry::sync("xml_get_error_code", &[PARSER], bindings::error_code),
        "int",
    ),
    (
        FunctionEntry::sync(
            "xml_error_string",
            &[Param::new("error_code", ParamType::Int)],
            errors::error_string,
        ),
        "?string",
    ),
    (
        FunctionEntry::sync("xml_get_current_line_number", &[PARSER], bindings::line),
        "int",
    ),
    (
        FunctionEntry::sync("xml_get_current_column_number", &[PARSER], bindings::column),
        "int",
    ),
    (
        FunctionEntry::sync("xml_get_current_byte_index", &[PARSER], bindings::byte),
        "int",
    ),
    (
        FunctionEntry::sync("xml_parser_free", &[PARSER], bindings::free),
        "bool",
    ),
    (
        FunctionEntry::sync(
            "xml_parser_set_option",
            &[PARSER, PARSE_OPTIONS, VALUE],
            bindings::set_option,
        ),
        "bool",
    ),
    (
        FunctionEntry::sync(
            "xml_parser_get_option",
            &[PARSER, PARSE_OPTIONS],
            bindings::get_option,
        ),
        "string|int|bool",
    ),
];

const CONSTANTS: &[(&str, i64)] = &[
    ("XML_ERROR_NONE", 0),
    ("XML_ERROR_NO_MEMORY", 1),
    ("XML_ERROR_SYNTAX", 2),
    ("XML_ERROR_NO_ELEMENTS", 3),
    ("XML_ERROR_INVALID_TOKEN", 4),
    ("XML_ERROR_UNCLOSED_TOKEN", 5),
    ("XML_ERROR_PARTIAL_CHAR", 6),
    ("XML_ERROR_TAG_MISMATCH", 7),
    ("XML_ERROR_DUPLICATE_ATTRIBUTE", 8),
    ("XML_ERROR_JUNK_AFTER_DOC_ELEMENT", 9),
    ("XML_ERROR_PARAM_ENTITY_REF", 10),
    ("XML_ERROR_UNDEFINED_ENTITY", 11),
    ("XML_ERROR_RECURSIVE_ENTITY_REF", 12),
    ("XML_ERROR_ASYNC_ENTITY", 13),
    ("XML_ERROR_BAD_CHAR_REF", 14),
    ("XML_ERROR_BINARY_ENTITY_REF", 15),
    ("XML_ERROR_ATTRIBUTE_EXTERNAL_ENTITY_REF", 16),
    ("XML_ERROR_MISPLACED_XML_PI", 17),
    ("XML_ERROR_UNKNOWN_ENCODING", 18),
    ("XML_ERROR_INCORRECT_ENCODING", 19),
    ("XML_ERROR_UNCLOSED_CDATA_SECTION", 20),
    ("XML_ERROR_EXTERNAL_ENTITY_HANDLING", 21),
    ("XML_OPTION_CASE_FOLDING", 1),
    ("XML_OPTION_TARGET_ENCODING", 2),
    ("XML_OPTION_SKIP_TAGSTART", 3),
    ("XML_OPTION_SKIP_WHITE", 4),
    ("XML_OPTION_PARSE_HUGE", 5),
];
