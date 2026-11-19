//! The constants of ext/standard: the flags its functions take, and the
//! mathematical ones.
//!
//! Every flag is a Rust constant as well, so the code that reads a flag
//! argument names it the way PHP code does: `flags & SORT_FLAG_CASE` rather
//! than `flags & 8`. Each flag is turned into an enum where its argument is
//! read, so no other code sees the integers.

use std::f64::consts;

use phera_extension_api::Registry;

/// `str_pad`'s `$pad_type`.
pub(crate) const STR_PAD_LEFT: i64 = 0;
pub(crate) const STR_PAD_RIGHT: i64 = 1;
pub(crate) const STR_PAD_BOTH: i64 = 2;

/// The `$flags` of `htmlspecialchars` and `htmlspecialchars_decode`. The
/// quote flags are two bits; `ENT_HTML401` is the absence of the two
/// document bits, which `ENT_HTML5` sets both of.
pub(crate) const ENT_COMPAT: i64 = 2;
pub(crate) const ENT_QUOTES: i64 = 3;
pub(crate) const ENT_NOQUOTES: i64 = 0;
pub(crate) const ENT_IGNORE: i64 = 4;
pub(crate) const ENT_SUBSTITUTE: i64 = 8;
pub(crate) const ENT_HTML401: i64 = 0;
pub(crate) const ENT_XML1: i64 = 16;
pub(crate) const ENT_XHTML: i64 = 32;
pub(crate) const ENT_HTML5: i64 = 48;
pub(crate) const ENT_DISALLOWED: i64 = 128;

/// `round`'s `$mode`, in its integer form. PHP 8.4 added the
/// `RoundingMode` enum, whose four other cases have no constant.
pub(crate) const PHP_ROUND_HALF_UP: i64 = 1;
pub(crate) const PHP_ROUND_HALF_DOWN: i64 = 2;
pub(crate) const PHP_ROUND_HALF_EVEN: i64 = 3;
pub(crate) const PHP_ROUND_HALF_ODD: i64 = 4;

/// The `$flags` of `sort` and its relatives: one comparison, optionally
/// with `SORT_FLAG_CASE` added.
pub(crate) const SORT_REGULAR: i64 = 0;
pub(crate) const SORT_NUMERIC: i64 = 1;
pub(crate) const SORT_STRING: i64 = 2;
pub(crate) const SORT_LOCALE_STRING: i64 = 5;
pub(crate) const SORT_NATURAL: i64 = 6;
pub(crate) const SORT_FLAG_CASE: i64 = 8;

/// `array_multisort`'s order arguments; registered for code that names them.
const SORT_ASC: i64 = 4;
const SORT_DESC: i64 = 3;

/// `array_filter`'s `$mode`; 0, the default, passes the value only.
pub(crate) const ARRAY_FILTER_USE_BOTH: i64 = 1;
pub(crate) const ARRAY_FILTER_USE_KEY: i64 = 2;

/// Flags of functions that live elsewhere: `array_change_key_case`,
/// `extract` (a VM intrinsic) and `get_html_translation_table`.
const OTHER_FLAGS: &[(&str, i64)] = &[
    ("CASE_LOWER", 0),
    ("CASE_UPPER", 1),
    ("EXTR_OVERWRITE", 0),
    ("EXTR_SKIP", 1),
    ("EXTR_PREFIX_SAME", 2),
    ("EXTR_PREFIX_ALL", 3),
    ("EXTR_PREFIX_INVALID", 4),
    ("EXTR_PREFIX_IF_EXISTS", 5),
    ("EXTR_IF_EXISTS", 6),
    ("EXTR_REFS", 256),
    ("HTML_SPECIALCHARS", 0),
    ("HTML_ENTITIES", 1),
];

/// PHP's `M_*` constants are the C library's, so they are the same doubles
/// as Rust's where Rust has one. The three Rust has no stable constant for
/// are spelled as their exact bits.
const MATH: &[(&str, f64)] = &[
    ("M_PI", consts::PI),
    ("M_E", consts::E),
    ("M_LOG2E", consts::LOG2_E),
    ("M_LOG10E", consts::LOG10_E),
    ("M_LN2", consts::LN_2),
    ("M_LN10", consts::LN_10),
    ("M_PI_2", consts::FRAC_PI_2),
    ("M_PI_4", consts::FRAC_PI_4),
    ("M_1_PI", consts::FRAC_1_PI),
    ("M_2_PI", consts::FRAC_2_PI),
    ("M_SQRTPI", f64::from_bits(0x3ffc5bf891b4ef6b)),
    ("M_2_SQRTPI", consts::FRAC_2_SQRT_PI),
    ("M_SQRT2", consts::SQRT_2),
    ("M_SQRT3", f64::from_bits(0x3ffbb67ae8584caa)),
    ("M_SQRT1_2", consts::FRAC_1_SQRT_2),
    ("M_LNPI", f64::from_bits(0x3ff250d048e7a1bd)),
    ("M_EULER", f64::from_bits(0x3fe2788cfc6fb619)),
];

pub(crate) fn register(registry: &mut Registry) {
    let flags = [
        ("STR_PAD_LEFT", STR_PAD_LEFT),
        ("STR_PAD_RIGHT", STR_PAD_RIGHT),
        ("STR_PAD_BOTH", STR_PAD_BOTH),
        ("ENT_COMPAT", ENT_COMPAT),
        ("ENT_QUOTES", ENT_QUOTES),
        ("ENT_NOQUOTES", ENT_NOQUOTES),
        ("ENT_IGNORE", ENT_IGNORE),
        ("ENT_SUBSTITUTE", ENT_SUBSTITUTE),
        ("ENT_HTML401", ENT_HTML401),
        ("ENT_XML1", ENT_XML1),
        ("ENT_XHTML", ENT_XHTML),
        ("ENT_HTML5", ENT_HTML5),
        ("ENT_DISALLOWED", ENT_DISALLOWED),
        ("PHP_ROUND_HALF_UP", PHP_ROUND_HALF_UP),
        ("PHP_ROUND_HALF_DOWN", PHP_ROUND_HALF_DOWN),
        ("PHP_ROUND_HALF_EVEN", PHP_ROUND_HALF_EVEN),
        ("PHP_ROUND_HALF_ODD", PHP_ROUND_HALF_ODD),
        ("SORT_REGULAR", SORT_REGULAR),
        ("SORT_NUMERIC", SORT_NUMERIC),
        ("SORT_STRING", SORT_STRING),
        ("SORT_LOCALE_STRING", SORT_LOCALE_STRING),
        ("SORT_NATURAL", SORT_NATURAL),
        ("SORT_FLAG_CASE", SORT_FLAG_CASE),
        ("SORT_ASC", SORT_ASC),
        ("SORT_DESC", SORT_DESC),
        ("ARRAY_FILTER_USE_BOTH", ARRAY_FILTER_USE_BOTH),
        ("ARRAY_FILTER_USE_KEY", ARRAY_FILTER_USE_KEY),
    ];
    let locale_deprecated = registry
        .level()
        .has(phera_extension_api::Change::SortLocaleStringDeprecated);
    for (name, value) in flags.into_iter().chain(OTHER_FLAGS.iter().copied()) {
        let deprecation = (locale_deprecated && name == "SORT_LOCALE_STRING").then_some(
            "Constant SORT_LOCALE_STRING is deprecated since 8.6, use one of the Collator::*sort*() methods instead",
        );
        registry.register_constant(name, value.into(), deprecation);
    }
    for &(name, value) in MATH {
        registry.constant(name, value);
    }
}
