//! PHP constants for implemented string, math and array operations.

use phera_extension_api::Registry;

/// Exact binary64 constants used by PHP.
pub(crate) fn register(registry: &mut Registry) {
    for (name, value) in [
        ("STR_PAD_LEFT", 0i64),
        ("STR_PAD_RIGHT", 1),
        ("STR_PAD_BOTH", 2),
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
    ] {
        registry.constant(name, value);
    }
    registry.constant("M_PI", f64::from_bits(0x400921fb54442d18));
    registry.constant("M_E", f64::from_bits(0x4005bf0a8b145769));
    registry.constant("M_LOG2E", f64::from_bits(0x3ff71547652b82fe));
    registry.constant("M_LOG10E", f64::from_bits(0x3fdbcb7b1526e50e));
    registry.constant("M_LN2", f64::from_bits(0x3fe62e42fefa39ef));
    registry.constant("M_LN10", f64::from_bits(0x40026bb1bbb55516));
    registry.constant("M_PI_2", f64::from_bits(0x3ff921fb54442d18));
    registry.constant("M_PI_4", f64::from_bits(0x3fe921fb54442d18));
    registry.constant("M_1_PI", f64::from_bits(0x3fd45f306dc9c883));
    registry.constant("M_2_PI", f64::from_bits(0x3fe45f306dc9c883));
    registry.constant("M_SQRTPI", f64::from_bits(0x3ffc5bf891b4ef6b));
    registry.constant("M_2_SQRTPI", f64::from_bits(0x3ff20dd750429b6d));
    registry.constant("M_SQRT2", f64::from_bits(0x3ff6a09e667f3bcd));
    registry.constant("M_SQRT3", f64::from_bits(0x3ffbb67ae8584caa));
    registry.constant("M_SQRT1_2", f64::from_bits(0x3fe6a09e667f3bcd));
    registry.constant("M_LNPI", f64::from_bits(0x3ff250d048e7a1bd));
    registry.constant("M_EULER", f64::from_bits(0x3fe2788cfc6fb619));
    registry.constant("PHP_ROUND_HALF_UP", 1i64);
    registry.constant("PHP_ROUND_HALF_DOWN", 2i64);
    registry.constant("PHP_ROUND_HALF_EVEN", 3i64);
    registry.constant("PHP_ROUND_HALF_ODD", 4i64);
    registry.constant("SORT_REGULAR", 0i64);
    registry.constant("SORT_NUMERIC", 1i64);
    registry.constant("SORT_STRING", 2i64);
    registry.constant("SORT_FLAG_CASE", 8i64);
    registry.constant("SORT_LOCALE_STRING", 5i64);
    registry.constant("SORT_NATURAL", 6i64);
    registry.constant("SORT_ASC", 4i64);
    registry.constant("SORT_DESC", 3i64);
    registry.constant("ARRAY_FILTER_USE_KEY", 2i64);
    registry.constant("ARRAY_FILTER_USE_BOTH", 1i64);
}
