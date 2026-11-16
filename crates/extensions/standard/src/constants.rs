use phera_extension_api::Registry;
/// Exact binary64 constants used by PHP.
pub(crate) fn register(r: &mut Registry) {
    r.constant("M_PI", f64::from_bits(0x400921fb54442d18));
    r.constant("M_E", f64::from_bits(0x4005bf0a8b145769));
    r.constant("M_LOG2E", f64::from_bits(0x3ff71547652b82fe));
    r.constant("M_LOG10E", f64::from_bits(0x3fdbcb7b1526e50e));
    r.constant("M_LN2", f64::from_bits(0x3fe62e42fefa39ef));
    r.constant("M_LN10", f64::from_bits(0x40026bb1bbb55516));
    r.constant("M_PI_2", f64::from_bits(0x3ff921fb54442d18));
    r.constant("M_PI_4", f64::from_bits(0x3fe921fb54442d18));
    r.constant("M_1_PI", f64::from_bits(0x3fd45f306dc9c883));
    r.constant("M_2_PI", f64::from_bits(0x3fe45f306dc9c883));
    r.constant("M_SQRTPI", f64::from_bits(0x3ffc5bf891b4ef6b));
    r.constant("M_2_SQRTPI", f64::from_bits(0x3ff20dd750429b6d));
    r.constant("M_SQRT2", f64::from_bits(0x3ff6a09e667f3bcd));
    r.constant("M_SQRT3", f64::from_bits(0x3ffbb67ae8584caa));
    r.constant("M_SQRT1_2", f64::from_bits(0x3fe6a09e667f3bcd));
    r.constant("M_LNPI", f64::from_bits(0x3ff250d048e7a1bd));
    r.constant("M_EULER", f64::from_bits(0x3fe2788cfc6fb619));
    r.constant("PHP_ROUND_HALF_UP", 1i64);
    r.constant("PHP_ROUND_HALF_DOWN", 2i64);
    r.constant("PHP_ROUND_HALF_EVEN", 3i64);
    r.constant("PHP_ROUND_HALF_ODD", 4i64);
    r.constant("SORT_REGULAR", 0i64);
    r.constant("SORT_NUMERIC", 1i64);
    r.constant("SORT_STRING", 2i64);
    r.constant("SORT_FLAG_CASE", 8i64);
    r.constant("SORT_LOCALE_STRING", 5i64);
    r.constant("SORT_NATURAL", 6i64);
    r.constant("SORT_ASC", 4i64);
    r.constant("SORT_DESC", 3i64);
    r.constant("ARRAY_FILTER_USE_KEY", 2i64);
    r.constant("ARRAY_FILTER_USE_BOTH", 1i64);
}
