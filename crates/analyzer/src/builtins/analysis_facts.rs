//! `ENT_QUOTES | ENT_HTML5` is an integer even when the stub uses UNKNOWN.

use crate::{index::Index, types::ValueType};

pub fn refine(index: &mut Index) {
    for (name, value) in [
        ("ENT_QUOTES", 3),
        ("ENT_HTML5", 48),
        ("ENT_SUBSTITUTE", 8),
        ("ENT_COMPAT", 2),
        ("ENT_NOQUOTES", 0),
        ("ENT_DISALLOWED", 128),
        ("ENT_HTML401", 0),
        ("ENT_XML1", 16),
        ("ENT_XHTML", 32),
        ("JSON_THROW_ON_ERROR", 4194304),
        ("JSON_UNESCAPED_SLASHES", 64),
        ("JSON_UNESCAPED_UNICODE", 256),
        ("PREG_SPLIT_NO_EMPTY", 1),
        ("PREG_SPLIT_DELIM_CAPTURE", 2),
        ("PREG_SPLIT_OFFSET_CAPTURE", 4),
        ("PREG_UNMATCHED_AS_NULL", 512),
        ("PREG_OFFSET_CAPTURE", 256),
        ("PREG_PATTERN_ORDER", 1),
        ("PREG_SET_ORDER", 2),
        ("LIBXML_NOERROR", 32),
        ("LIBXML_NOWARNING", 64),
        ("LIBXML_NONET", 2048),
        ("LIBXML_HTML_NOIMPLIED", 8192),
        ("LIBXML_HTML_NODEFDTD", 4),
    ] {
        if let Some(constant) = index.constants.get_mut(name) {
            constant.ty = ValueType::IntLiteral(value);
        }
    }
    for name in [
        "grapheme_strpos",
        "grapheme_stripos",
        "grapheme_strrpos",
        "grapheme_strripos",
        "grapheme_substr",
        "grapheme_strlen",
        "iconv_strlen",
        "iconv_substr",
        "preg_split",
        "mb_detect_encoding",
    ] {
        super::runtime_signatures::literal_false(index, name);
    }
}
