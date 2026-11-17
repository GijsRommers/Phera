//! PHP regular expressions: `preg_match('/hello/i', 'HELLO') === 1`.

mod cache;
mod callable;
mod callbacks;
mod compile;
mod errors;
mod flags;
mod grep;
mod matches;
mod matching;
mod pattern;
mod quote;
mod replace;
mod replacement;
mod scan;
mod signatures;
mod split;
mod transform;

use errors::Error;
use phera_extension_api::{Extension, Registry};

/// PHP's PCRE extension, backed exclusively by the safe PCRE2 API.
pub struct Pcre;

impl Extension for Pcre {
    fn name(&self) -> &'static str {
        "pcre"
    }

    fn register(&self, registry: &mut Registry) {
        signatures::register(registry);
        for &(name, value) in CONSTANTS {
            registry.constant(name, value);
        }
        // pcre2::version() reads stale generated bindings (10.45), not the linked library.
        // These values come from the pinned pcre2-sys 0.2.10 bundled source header.
        registry.constant("PCRE_VERSION_MAJOR", 10_i64);
        registry.constant("PCRE_VERSION_MINOR", 46_i64);
        registry.constant("PCRE_VERSION", "10.46 2025-08-27");
        registry.constant("PCRE_JIT_SUPPORT", pcre2::is_jit_available());
    }
}

const CONSTANTS: &[(&str, i64)] = &[
    ("PREG_PATTERN_ORDER", flags::PATTERN_ORDER),
    ("PREG_SET_ORDER", flags::SET_ORDER),
    ("PREG_OFFSET_CAPTURE", flags::OFFSET_CAPTURE),
    ("PREG_UNMATCHED_AS_NULL", flags::UNMATCHED_AS_NULL),
    ("PREG_SPLIT_NO_EMPTY", flags::SPLIT_NO_EMPTY),
    ("PREG_SPLIT_DELIM_CAPTURE", flags::SPLIT_DELIM_CAPTURE),
    ("PREG_SPLIT_OFFSET_CAPTURE", flags::SPLIT_OFFSET_CAPTURE),
    ("PREG_GREP_INVERT", flags::GREP_INVERT),
    ("PREG_NO_ERROR", Error::None as i64),
    ("PREG_INTERNAL_ERROR", Error::Internal as i64),
    ("PREG_BACKTRACK_LIMIT_ERROR", Error::Backtrack as i64),
    ("PREG_RECURSION_LIMIT_ERROR", Error::Recursion as i64),
    ("PREG_BAD_UTF8_ERROR", Error::Utf8 as i64),
    ("PREG_BAD_UTF8_OFFSET_ERROR", Error::Utf8Offset as i64),
    ("PREG_JIT_STACKLIMIT_ERROR", Error::JitStack as i64),
];
