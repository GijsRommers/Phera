//! The `$flags` bits of `json_encode` and `json_decode`.
//!
//! ```php
//! json_encode('a/é', JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);  // "a/é"
//! json_decode('[12345678901234567890]', true, 512, JSON_BIGINT_AS_STRING);
//! // ['12345678901234567890']
//! ```
//!
//! Encoding and decoding read the same low bits differently: 1 is
//! `JSON_HEX_TAG` to one and `JSON_OBJECT_AS_ARRAY` to the other. Bits that
//! do not apply are ignored, as in PHP.

pub const HEX_TAG: i64 = 1;
pub const HEX_AMP: i64 = 2;
pub const HEX_APOS: i64 = 4;
pub const HEX_QUOT: i64 = 8;
pub const FORCE_OBJECT: i64 = 16;
pub const NUMERIC_CHECK: i64 = 32;
pub const UNESCAPED_SLASHES: i64 = 64;
pub const PRETTY_PRINT: i64 = 128;
pub const UNESCAPED_UNICODE: i64 = 256;
pub const PARTIAL_OUTPUT_ON_ERROR: i64 = 512;
pub const PRESERVE_ZERO_FRACTION: i64 = 1024;
pub const UNESCAPED_LINE_TERMINATORS: i64 = 2048;
pub const OBJECT_AS_ARRAY: i64 = 1;
pub const BIGINT_AS_STRING: i64 = 2;
pub const INVALID_UTF8_IGNORE: i64 = 1 << 20;
pub const INVALID_UTF8_SUBSTITUTE: i64 = 1 << 21;
pub const THROW_ON_ERROR: i64 = 1 << 22;

/// What to do with bytes that are not UTF-8, in encoding and decoding.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum InvalidUtf8 {
    /// An error: `JSON_ERROR_UTF8`.
    #[default]
    Fail,
    /// `JSON_INVALID_UTF8_IGNORE`: leave them out.
    Ignore,
    /// `JSON_INVALID_UTF8_SUBSTITUTE`: write U+FFFD instead.
    Substitute,
}

impl InvalidUtf8 {
    /// When both bits are set, encoding ignores and decoding substitutes,
    /// as in php-src.
    fn for_encoding(bits: i64) -> Self {
        if bits & INVALID_UTF8_IGNORE != 0 {
            Self::Ignore
        } else if bits & INVALID_UTF8_SUBSTITUTE != 0 {
            Self::Substitute
        } else {
            Self::Fail
        }
    }

    fn for_decoding(bits: i64) -> Self {
        if bits & INVALID_UTF8_SUBSTITUTE != 0 {
            Self::Substitute
        } else if bits & INVALID_UTF8_IGNORE != 0 {
            Self::Ignore
        } else {
            Self::Fail
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct EncodeFlags {
    pub hex_tag: bool,
    pub hex_amp: bool,
    pub hex_apos: bool,
    pub hex_quot: bool,
    pub force_object: bool,
    pub numeric_check: bool,
    pub unescaped_slashes: bool,
    pub pretty_print: bool,
    pub unescaped_unicode: bool,
    pub partial_output: bool,
    pub preserve_zero_fraction: bool,
    pub unescaped_line_terminators: bool,
    pub invalid_utf8: InvalidUtf8,
    pub throw_on_error: bool,
}

impl EncodeFlags {
    pub fn from_bits(bits: i64) -> Self {
        let set = |flag: i64| bits & flag != 0;
        Self {
            hex_tag: set(HEX_TAG),
            hex_amp: set(HEX_AMP),
            hex_apos: set(HEX_APOS),
            hex_quot: set(HEX_QUOT),
            force_object: set(FORCE_OBJECT),
            numeric_check: set(NUMERIC_CHECK),
            unescaped_slashes: set(UNESCAPED_SLASHES),
            pretty_print: set(PRETTY_PRINT),
            unescaped_unicode: set(UNESCAPED_UNICODE),
            partial_output: set(PARTIAL_OUTPUT_ON_ERROR),
            preserve_zero_fraction: set(PRESERVE_ZERO_FRACTION),
            unescaped_line_terminators: set(UNESCAPED_LINE_TERMINATORS),
            invalid_utf8: InvalidUtf8::for_encoding(bits),
            throw_on_error: set(THROW_ON_ERROR),
        }
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct DecodeFlags {
    /// Objects become arrays: `JSON_OBJECT_AS_ARRAY`, or `$associative`.
    pub associative: bool,
    pub bigint_as_string: bool,
    pub invalid_utf8: InvalidUtf8,
    pub throw_on_error: bool,
}

impl DecodeFlags {
    pub fn from_bits(bits: i64) -> Self {
        Self {
            associative: bits & OBJECT_AS_ARRAY != 0,
            bigint_as_string: bits & BIGINT_AS_STRING != 0,
            invalid_utf8: InvalidUtf8::for_decoding(bits),
            throw_on_error: bits & THROW_ON_ERROR != 0,
        }
    }
}
