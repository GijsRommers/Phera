//! PHP JSON encoding, parsing and request-local errors.
//! JsonSerializable dispatch awaits VM method-call integration.
//! Non-associative decoding awaits a VM-owned stdClass factory.

mod decode;
mod encode;
mod error;
mod flags;
mod string;

use phera_extension_api::{Extension, FunctionEntry, Param, ParamType, Registry};

/// Registers PHP JSON functions and their stable option/error constants.
pub struct Json;

impl Extension for Json {
    fn name(&self) -> &'static str {
        "json"
    }

    fn register(&self, registry: &mut Registry) {
        const ENCODE: &[Param] = &[
            Param::new("value", ParamType::Mixed),
            Param::new("flags", ParamType::Int).optional(),
            Param::new("depth", ParamType::Int).optional(),
        ];
        registry.function(FunctionEntry::sync(
            "json_encode",
            ENCODE,
            encode::json_encode,
        ));

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
    ("JSON_FORCE_OBJECT", 16),
    ("JSON_NUMERIC_CHECK", 32),
    ("JSON_UNESCAPED_SLASHES", flags::UNESCAPED_SLASHES),
    ("JSON_PRETTY_PRINT", 128),
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
    ("JSON_OBJECT_AS_ARRAY", 1),
    ("JSON_BIGINT_AS_STRING", 2),
    ("JSON_INVALID_UTF8_IGNORE", flags::INVALID_UTF8_IGNORE),
    (
        "JSON_INVALID_UTF8_SUBSTITUTE",
        flags::INVALID_UTF8_SUBSTITUTE,
    ),
    ("JSON_THROW_ON_ERROR", flags::THROW_ON_ERROR),
    ("JSON_ERROR_NONE", 0),
    ("JSON_ERROR_DEPTH", 1),
    ("JSON_ERROR_STATE_MISMATCH", 2),
    ("JSON_ERROR_CTRL_CHAR", 3),
    ("JSON_ERROR_SYNTAX", 4),
    ("JSON_ERROR_UTF8", 5),
    ("JSON_ERROR_RECURSION", 6),
    ("JSON_ERROR_INF_OR_NAN", 7),
    ("JSON_ERROR_UNSUPPORTED_TYPE", 8),
    ("JSON_ERROR_INVALID_PROPERTY_NAME", 9),
    ("JSON_ERROR_UTF16", 10),
    ("JSON_ERROR_NON_BACKED_ENUM", 11),
];
