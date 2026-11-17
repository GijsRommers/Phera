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
    ("PREG_PATTERN_ORDER", 1),
    ("PREG_SET_ORDER", 2),
    ("PREG_OFFSET_CAPTURE", 256),
    ("PREG_UNMATCHED_AS_NULL", 512),
    ("PREG_SPLIT_NO_EMPTY", 1),
    ("PREG_SPLIT_DELIM_CAPTURE", 2),
    ("PREG_SPLIT_OFFSET_CAPTURE", 4),
    ("PREG_GREP_INVERT", 1),
    ("PREG_NO_ERROR", 0),
    ("PREG_INTERNAL_ERROR", 1),
    ("PREG_BACKTRACK_LIMIT_ERROR", 2),
    ("PREG_RECURSION_LIMIT_ERROR", 3),
    ("PREG_BAD_UTF8_ERROR", 4),
    ("PREG_BAD_UTF8_OFFSET_ERROR", 5),
    ("PREG_JIT_STACKLIMIT_ERROR", 6),
];
