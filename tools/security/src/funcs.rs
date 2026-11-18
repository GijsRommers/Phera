//! Hostile-argument campaign: calls native functions with wrong types,
//! boundary integers, huge repeat counts, NaN/INF and self-referential
//! arrays. The function list is the union of the registry names found in
//! crates/extensions (see README) and a curated set of the riskiest ones.

use crate::rng::Rng;

/// Curated high-risk functions: decoders, matchers, formatters, allocators.
pub const RISKY: &[&str] = &[
    "json_decode", "json_encode", "preg_match", "preg_match_all", "preg_replace",
    "preg_replace_callback", "preg_split", "preg_grep", "preg_quote", "sprintf", "vsprintf",
    "printf", "fprintf", "number_format", "str_repeat", "str_pad", "strtr", "substr",
    "substr_count", "substr_replace", "str_replace", "str_ireplace", "strpos", "stripos",
    "strrpos", "str_contains", "explode", "implode", "str_split", "chunk_split", "wordwrap",
    "nl2br", "addcslashes", "addslashes", "quotemeta", "base64_encode", "base64_decode",
    "bin2hex", "hex2bin", "urlencode", "urldecode", "rawurlencode", "rawurldecode",
    "htmlentities", "htmlspecialchars", "html_entity_decode", "strip_tags", "ucwords",
    "lcfirst", "ucfirst", "strtolower", "strtoupper", "strrev", "trim", "ltrim", "rtrim",
    "similar_text", "levenshtein", "metaphone", "soundex", "count_chars", "str_word_count",
    "range", "array_fill", "array_fill_keys", "array_chunk", "array_slice", "array_splice",
    "array_pad", "array_combine", "array_merge", "array_merge_recursive", "array_replace",
    "array_replace_recursive", "array_keys", "array_values", "array_flip", "array_unique",
    "array_count_values", "array_sum", "array_product", "array_filter", "array_map",
    "array_walk", "array_walk_recursive", "array_reduce", "array_search", "in_array",
    "array_key_exists", "array_key_first", "array_key_last", "array_pop", "array_push",
    "array_shift", "array_unshift", "array_rand", "shuffle", "sort", "rsort", "usort",
    "uasort", "uksort", "asort", "arsort", "ksort", "krsort", "natsort", "natcasesort",
    "array_reverse", "array_intersect", "array_intersect_key", "array_diff",
    "array_diff_key", "array_union", "compact", "extract", "count", "sizeof",
    "serialize", "unserialize", "var_dump", "var_export", "print_r", "json_last_error",
    "json_last_error_msg", "pack", "unpack", "ord", "chr", "bindec", "hexdec", "octdec",
    "dechex", "decbin", "decoct", "base_convert", "intdiv", "fdiv", "pow", "sqrt", "abs",
    "max", "min", "floor", "ceil", "round", "intval", "floatval", "strval", "boolval",
    "settype", "gettype", "is_int", "is_float", "is_string", "is_array", "is_object",
    "is_callable", "function_exists", "call_user_func", "call_user_func_array",
    "forward_static_call", "mb_strlen", "mb_substr", "mb_strtolower", "mb_strtoupper",
    "mb_convert_case", "mb_strimwidth", "mb_encode_numericentity", "date", "strtotime",
    "strftime", "gmdate", "microtime", "hash", "hash_hmac", "md5", "sha1", "crc32",
    "filter_var", "filter_input", "ctype_digit", "ctype_alpha", "lcg_value", "random_int",
    "random_bytes", "uniqid", "password_hash", "password_verify",
];

/// Argument templates. Each is PHP source for the argument list.
pub const ARGS: &[&str] = &[
    "",
    "1",
    "-1",
    "0",
    "PHP_INT_MAX",
    "PHP_INT_MIN",
    "PHP_INT_MAX + 1",
    "-9223372036854775809",
    "9223372036854775808",
    "NAN",
    "INF",
    "-INF",
    "1e308",
    "1.5",
    "'x'",
    "''",
    "true",
    "false",
    "null",
    "[]",
    "[1,2,3]",
    "'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'",
    "str_repeat('a', 100000)",
    "range(1, 1000)",
    "array_fill(0, 1000, 'x')",
    "[PHP_INT_MAX => 'x', PHP_INT_MIN => 'y', '9223372036854775808' => 'z', '1.5' => 'w', 1.9 => 'v', true => 't', null => 'n']",
    "$GLOBALS['selfref']",
    "str_repeat('a', 10) . chr(0) . str_repeat('b', 10)",
    "\"\\u{10ffff}\\u{0}\"",
    "new stdClass",
    "function ($x) { return $x; }",
    "'strlen'",
    "[new stdClass, 'nope']",
    "1, 2, 3, 4, 5, 6, 7, 8",
    "str_repeat(',', 100)",
    "'%', str_repeat('s', 100)",
    "'/%s/' , str_repeat('a', 1000)",
    "'/(\\d+)/', str_repeat('1', 100000)",
    "'/(a+)+$/', str_repeat('a', 30) . 'b'",
    "'{}', 1",
    "'a', PHP_INT_MAX",
    "'a', -1",
    "0, PHP_INT_MAX, 'x'",
    "'x', 0, PHP_INT_MAX",
    "512",
    "513",
    "-512",
    "JSON_THROW_ON_ERROR",
    "str_repeat('[', 600) . '1' . str_repeat(']', 600)",
    "'{\"a\":' . str_repeat('1,', 100) . '1}'",
    "512, JSON_INVALID_UTF8_IGNORE",
    "'/./u', \"\\xff\\xfe\"",
    "'/(?<name>a)/', 'a'",
    "'/(?P<n>a)\\k<n>/', 'aa'",
    "['a' => 1], ['b' => 2]",
    "['x'], ['y'], ['z']",
    "SORT_NUMERIC, range(1, 100)",
    "fn($a, $b) => $a <=> $b, range(1, 100)",
    "[$GLOBALS['selfref']], fn($x) => $x",
];

/// Self-referential array snippet prepended to the script.
pub const PRELUDE: &str = "<?php $GLOBALS['selfref'] = []; $GLOBALS['selfref']['self'] = &$GLOBALS['selfref'];\n";

/// Generates one case per (function, arg-template) pair, shuffled.
pub fn cases(rng: &mut Rng) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for func in RISKY {
        for args in ARGS {
            out.push((
                format!("{func}({args})"),
                format!("{PRELUDE}try {{ {func}({args}); }} catch (Throwable $e) {{ }} echo 'ok';\n"),
            ));
        }
    }
    // shuffle deterministically
    for i in (1..out.len()).rev() {
        let j = rng.below(i + 1);
        out.swap(i, j);
    }
    out
}
