//! Compare PHP values, changed references and diagnostics with PHP's intl module.

use phera_extension_api::{PhpArray, Value};

mod cases;

mod matrix;
use matrix::Matrix;

#[test]
fn normalization_and_errors() {
    let mut matrix = Matrix::new();
    let strings = [
        "",
        "\0",
        "a\0",
        "ñ",
        "n\u{303}",
        "Å",
        "Å",
        "Ǆ",
        "῟",
        "ྋ",
        "ệ",
        "㌀",
        "각",
        "가",
        "👩‍💻",
        "Α\u{0345}",
        "\u{ad}",
        "ʼ",
        "ﬃ",
        "Straße",
        "Ａ",
        "Ω",
        "a\u{0315}\u{0300}b",
        "à\u{0315}",
        "a\u{034f}\u{0315}\u{0300}",
    ]
    .into_iter()
    .map(Value::from)
    .chain(
        [
            b"\xff".as_slice(),
            b"\xff\xff",
            b"\xe2\x82",
            b"\xf0\x90\x80",
            b"\xc0\x80",
            b"\xed\xa0\x80",
            b"a\xff",
        ]
        .map(|input| Value::from(input.to_vec())),
    );
    let functions = [
        "normalizer_normalize",
        "normalizer_is_normalized",
        "normalizer_get_raw_decomposition",
        "Normalizer::normalize",
        "Normalizer::isNormalized",
        "Normalizer::getRawDecomposition",
    ];
    for text in strings {
        for function in functions {
            matrix.case(function, std::slice::from_ref(&text));
            for form in [4, 8, 16, 32, 48, 0, 17, -1, i64::MAX] {
                matrix.case(function, &[text.clone(), Value::Int(form)]);
            }
        }
    }
    for function in functions {
        matrix.case(function, &[]);
        matrix.case(function, &[Value::Null, Value::Null]);
        matrix.case(function, &[Value::from(PhpArray::new())]);
        matrix.case(function, &[Value::from("x"), Value::from("x")]);
    }
    for code in [
        -129,
        -128,
        -119,
        -1,
        0,
        1,
        9,
        10,
        30,
        32,
        65536,
        65569,
        i64::MAX,
        1 << 32,
    ] {
        matrix.case("intl_error_name", &[Value::Int(code)]);
        matrix.case("intl_is_failure", &[Value::Int(code)]);
    }
    matrix.compare("intl-normalization");
}

#[test]
fn grapheme_boundaries_and_references() {
    let mut matrix = Matrix::new();
    let strings = [
        "",
        "ab\0c",
        "\r\nxy\r",
        "e\u{301}ñ",
        "👩‍💻🇧🇪",
        "क्ष",
        "👨‍👩‍👧‍👦",
        "세은",
        "ไทย",
    ]
    .map(Value::from)
    .into_iter()
    .chain(
        [
            b"\xff".as_slice(),
            b"\xff\xfe",
            b"\xe2\x82x",
            b"\xe2x\xff",
            b"a\xff\0b",
        ]
        .map(|input| Value::from(input.to_vec())),
    );
    for text in strings {
        matrix.case("grapheme_strlen", std::slice::from_ref(&text));
        for length in [i64::MIN, -3, -1, 0, 1, 2, i64::MAX] {
            matrix.case("grapheme_str_split", &[text.clone(), Value::Int(length)]);
            for offset in [i64::MIN, -6, -1, 0, 1, 2, 6, i64::MAX] {
                matrix.case(
                    "grapheme_substr",
                    &[text.clone(), Value::Int(offset), Value::Int(length)],
                );
            }
        }
        for unit in [0, 1, 2, 3] {
            for offset in [-2, 0, 1, 2, 6, i64::MAX] {
                for size in [-1, 0, 1, 2, 6, i64::MAX] {
                    matrix.record(
                        "grapheme_extract",
                        &[
                            text.clone(),
                            Value::Int(size),
                            Value::Int(unit),
                            Value::Int(offset),
                        ],
                        Some(Value::from("unmodified")),
                    );
                }
            }
        }
    }
    matrix.compare("intl-grapheme-boundaries");
}

#[test]
fn grapheme_search_and_distance() {
    let mut matrix = Matrix::new();
    let pairs = [
        ("", ""),
        ("", "x"),
        ("x", ""),
        ("a\0a", "\0"),
        ("é", "\0"),
        ("xééx", "e\u{301}"),
        ("aÉéa", "é"),
        ("aß", "ss"),
        ("àx", "a"),
        ("x\r\nx", "\r"),
        ("xyxyx", "xyx"),
        ("👩‍💻 a café", "cafe\u{301}"),
        ("far", "fAr"),
        ("Iıiİ", "i"),
        ("aﬃ", "ffi"),
    ];
    for (text, needle) in pairs {
        let text = Value::from(text);
        let needle = Value::from(needle);
        for offset in [i64::MIN, -3, -1, 0, 1, 3, i64::MAX] {
            for function in [
                "grapheme_strpos",
                "grapheme_strrpos",
                "grapheme_stripos",
                "grapheme_strripos",
            ] {
                matrix.case(
                    function,
                    &[text.clone(), needle.clone(), Value::Int(offset)],
                );
            }
        }
        for locale in ["", "tr_TR", "en_US"] {
            for before in [false, true] {
                for function in ["grapheme_strstr", "grapheme_stristr"] {
                    matrix.case(
                        function,
                        &[
                            text.clone(),
                            needle.clone(),
                            Value::Bool(before),
                            Value::from(locale),
                        ],
                    );
                }
            }
            for (insert, replace, delete) in [(1, 1, 1), (1, 3, 2), (0, 14, 10)] {
                matrix.case(
                    "grapheme_levenshtein",
                    &[
                        text.clone(),
                        needle.clone(),
                        Value::Int(insert),
                        Value::Int(replace),
                        Value::Int(delete),
                        Value::from(locale),
                    ],
                );
            }
        }
    }
    matrix.compare("intl-grapheme-search");
}
