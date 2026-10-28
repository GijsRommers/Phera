//! PHP's `ext/json`. Only scalar values exist so far, so this covers
//! `json_encode()` for scalars and the extension's constants.

mod encode;

use phera_extension_api::{Extension, Registry};

pub struct Json;

impl Extension for Json {
    fn name(&self) -> &'static str {
        "json"
    }

    fn register(&self, registry: &mut Registry) {
        registry.function("json_encode", encode::json_encode);

        for &(name, value) in CONSTANTS {
            registry.constant(name, value);
        }
    }
}

const CONSTANTS: &[(&str, i64)] = &[
    ("JSON_HEX_TAG", encode::HEX_TAG),
    ("JSON_HEX_AMP", encode::HEX_AMP),
    ("JSON_HEX_APOS", encode::HEX_APOS),
    ("JSON_HEX_QUOT", encode::HEX_QUOT),
    ("JSON_FORCE_OBJECT", 16),
    ("JSON_NUMERIC_CHECK", 32),
    ("JSON_UNESCAPED_SLASHES", encode::UNESCAPED_SLASHES),
    ("JSON_PRETTY_PRINT", 128),
    ("JSON_UNESCAPED_UNICODE", encode::UNESCAPED_UNICODE),
    (
        "JSON_PARTIAL_OUTPUT_ON_ERROR",
        encode::PARTIAL_OUTPUT_ON_ERROR,
    ),
    (
        "JSON_PRESERVE_ZERO_FRACTION",
        encode::PRESERVE_ZERO_FRACTION,
    ),
    (
        "JSON_UNESCAPED_LINE_TERMINATORS",
        encode::UNESCAPED_LINE_TERMINATORS,
    ),
    ("JSON_OBJECT_AS_ARRAY", 1),
    ("JSON_BIGINT_AS_STRING", 2),
    ("JSON_INVALID_UTF8_IGNORE", encode::INVALID_UTF8_IGNORE),
    (
        "JSON_INVALID_UTF8_SUBSTITUTE",
        encode::INVALID_UTF8_SUBSTITUTE,
    ),
    ("JSON_THROW_ON_ERROR", encode::THROW_ON_ERROR),
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
