//! PHP's JSON extension: `json_encode`, `json_decode`, `json_validate` and
//! the last-error functions.
//!
//! ```php
//! json_encode(['a' => 1.0, 'b' => 'é']);  // {"a":1,"b":"\u00e9"}
//! json_decode('{"a":1}', true);           // ['a' => 1]
//! json_last_error_msg();                  // "No error"
//! ```
//!
//! Calls JsonSerializable before falling back to a backed enum case’s value.

mod decode;
mod encode;
mod error;
mod flags;
mod recursion;
mod string;

use error::Error;
use phera_extension_api::{
    ClassEntry, Extension, FunctionEntry, MethodEntry, Param, ParamType, Registry,
};

/// Check JSON syntax without changing the request's `json_last_error()`.
pub fn valid_json(bytes: &[u8], depth: usize) -> bool {
    decode::validate(bytes, depth, 0).is_ok()
}

pub struct Json;

impl Extension for Json {
    fn name(&self) -> &'static str {
        "json"
    }

    fn register(&self, registry: &mut Registry) {
        // How a fork of a snapshot gets this extension's request state.
        registry.fork_state::<crate::recursion::Active>(phera_extension_api::fresh_state);
        registry.fork_state::<crate::error::Error>(phera_extension_api::fresh_state);
        registry.class(ClassEntry {
            methods: const { &[MethodEntry::required("jsonSerialize", &[]).tentative_return("mixed")] },
            ..ClassEntry::interface("JsonSerializable")
        });
        const ENCODE: &[Param] = &[
            Param::new("value", ParamType::Mixed),
            Param::new("flags", ParamType::Int).optional(),
            Param::new("depth", ParamType::Int).optional(),
        ];
        registry.function_with_callback_frame("json_encode", ENCODE, encode::json_encode);

        const DECODE: &[Param] = &[
            Param::new("json", ParamType::String),
            Param::new("associative", ParamType::Bool)
                .nullable()
                .optional(),
            Param::new("depth", ParamType::Int).optional(),
            Param::new("flags", ParamType::Int).optional(),
        ];

        const VALIDATE: &[Param] = &[
            Param::new("json", ParamType::String),
            Param::new("depth", ParamType::Int).optional(),
            Param::new("flags", ParamType::Int).optional(),
        ];
        registry.function(FunctionEntry::sync(
            "json_decode",
            DECODE,
            decode::json_decode,
        ));
        registry.function(FunctionEntry::sync(
            "json_validate",
            VALIDATE,
            decode::json_validate,
        ));
        registry.function(FunctionEntry::sync("json_last_error", &[], error::last));
        registry.function(FunctionEntry::sync(
            "json_last_error_msg",
            &[],
            error::message,
        ));
        for (name, return_type) in [
            ("json_encode", "string|false"),
            ("json_decode", "mixed"),
            ("json_validate", "bool"),
            ("json_last_error", "int"),
            ("json_last_error_msg", "string"),
        ] {
            registry.describe_function(name, return_type, None);
        }
        for &(name, value) in CONSTANTS {
            registry.constant(name, value);
        }
    }
}

const CONSTANTS: &[(&str, i64)] = &[
    ("JSON_HEX_TAG", flags::HEX_TAG),
    ("JSON_HEX_AMP", flags::HEX_AMP),
    ("JSON_HEX_APOS", flags::HEX_APOS),
    ("JSON_HEX_QUOT", flags::HEX_QUOT),
    ("JSON_FORCE_OBJECT", flags::FORCE_OBJECT),
    ("JSON_NUMERIC_CHECK", flags::NUMERIC_CHECK),
    ("JSON_UNESCAPED_SLASHES", flags::UNESCAPED_SLASHES),
    ("JSON_PRETTY_PRINT", flags::PRETTY_PRINT),
    ("JSON_UNESCAPED_UNICODE", flags::UNESCAPED_UNICODE),
    (
        "JSON_PARTIAL_OUTPUT_ON_ERROR",
        flags::PARTIAL_OUTPUT_ON_ERROR,
    ),
    ("JSON_PRESERVE_ZERO_FRACTION", flags::PRESERVE_ZERO_FRACTION),
    (
        "JSON_UNESCAPED_LINE_TERMINATORS",
        flags::UNESCAPED_LINE_TERMINATORS,
    ),
    ("JSON_OBJECT_AS_ARRAY", flags::OBJECT_AS_ARRAY),
    ("JSON_BIGINT_AS_STRING", flags::BIGINT_AS_STRING),
    ("JSON_INVALID_UTF8_IGNORE", flags::INVALID_UTF8_IGNORE),
    (
        "JSON_INVALID_UTF8_SUBSTITUTE",
        flags::INVALID_UTF8_SUBSTITUTE,
    ),
    ("JSON_THROW_ON_ERROR", flags::THROW_ON_ERROR),
    ("JSON_ERROR_NONE", Error::None as i64),
    ("JSON_ERROR_DEPTH", Error::Depth as i64),
    ("JSON_ERROR_STATE_MISMATCH", Error::State as i64),
    ("JSON_ERROR_CTRL_CHAR", Error::Control as i64),
    ("JSON_ERROR_SYNTAX", Error::Syntax as i64),
    ("JSON_ERROR_UTF8", Error::Utf8 as i64),
    ("JSON_ERROR_RECURSION", Error::Recursion as i64),
    ("JSON_ERROR_INF_OR_NAN", Error::Infinite as i64),
    ("JSON_ERROR_UNSUPPORTED_TYPE", Error::Unsupported as i64),
    ("JSON_ERROR_INVALID_PROPERTY_NAME", Error::Property as i64),
    ("JSON_ERROR_UTF16", Error::Utf16 as i64),
    ("JSON_ERROR_NON_BACKED_ENUM", Error::NonBackedEnum as i64),
];
