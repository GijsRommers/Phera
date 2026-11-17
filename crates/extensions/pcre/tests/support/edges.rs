//! Binary patterns, option scopes and callback containers supplement the broad tables.

use super::{Matrix, list, reference};
use phera_extension_api::{ArrayKey, PhpArray, Value};

pub fn cases(matrix: &mut Matrix) {
    for bytes in [
        vec![0xff],
        vec![b'a', 0xff],
        vec![0xc0, 0xaf],
        vec![0x80],
        vec![0xe2, 0x82],
        vec![0xed, 0xa0, 0x80],
        vec![0xf4, 0x90, 0x80, 0x80],
    ] {
        for modifier in [b"".as_slice(), b"u"] {
            for body in [
                bytes.clone(),
                [b"[".as_slice(), &bytes, b"]"].concat(),
                [b"\\Q".as_slice(), &bytes, b"\\E"].concat(),
            ] {
                let pattern = [b"/".as_slice(), &body, b"/", modifier].concat();
                matrix.case(
                    "preg_match",
                    vec![
                        Value::from(pattern),
                        Value::from(bytes.clone()),
                        reference(),
                        Value::Int(768),
                    ],
                );
            }
        }
    }
    for pattern in [
        r"/\C/u",
        r"/[\C]/u",
        r"/\Q\C\E/u",
        r"/\\C/u",
        "/(?#\\C)a/u",
        "/#\\C\na/xu",
        "/a# final comment/xA",
        r"/\Q/A",
        r"/\Q/",
        r"/\Q$/D",
        "/(?x)a(?-x)b/A",
        "/(?m)a$/D",
        "/(?m:a$)|b$/D",
        "/(?-m:a$)/mD",
        "/(?x)# $\na$/D",
        "/(?#$)a$/D",
        "/[$]$/D",
        "/[]$]$/D",
        "/[[:alpha:]$]/D",
        "/a(?R)?b/A",
        "/(?:|a(?R)?b)/",
        "/(a?)(?(R)b|c)/",
        "/(*CRLF)(?:|a)/",
        "/(?:|a\\K)/",
        "/(?=a)\\K/",
        "/é)/u",
        "\u{b}/a/",
        "/a/\u{b}",
    ] {
        for subject in [
            "",
            "a",
            "b",
            "ab",
            "aaabbb",
            "a\nb\n",
            "a\r\nb\r\n",
            "$",
            "a$",
            "\\C",
        ] {
            matrix.case(
                "preg_match_all",
                vec![
                    Value::from(pattern),
                    Value::from(subject),
                    reference(),
                    Value::Int(770),
                ],
            );
            matrix.case(
                "preg_replace",
                vec![
                    Value::from(pattern),
                    Value::from("X"),
                    Value::from(subject),
                    Value::Int(-1),
                    reference(),
                ],
            );
            matrix.case(
                "preg_split",
                vec![
                    Value::from(pattern),
                    Value::from(subject),
                    Value::Int(-1),
                    Value::Int(7),
                ],
            );
        }
    }
    let keyed = Value::from(
        [
            (ArrayKey::str("first"), Value::from("ab")),
            (ArrayKey::Int(4), Value::from("c")),
            (ArrayKey::str("bad"), Value::from(vec![0xff])),
        ]
        .into_iter()
        .collect::<PhpArray>(),
    );
    for patterns in [
        list([]),
        Value::from("/./u"),
        list([Value::from("/a/"), Value::from("/b/")]),
        list([Value::from("/a/"), Value::from("/(/")]),
    ] {
        for callback in [
            "upper",
            "length",
            "null_result",
            "array_result",
            "fail",
            "inspect",
        ] {
            for limit in [-1, 0, 1, 2, -2] {
                matrix.case(
                    "preg_replace_callback",
                    vec![
                        patterns.clone(),
                        Value::from(callback),
                        keyed.clone(),
                        Value::Int(limit),
                        reference(),
                        Value::Int(768),
                    ],
                );
            }
        }
    }
    for patterns in [
        PhpArray::new(),
        [
            (ArrayKey::str("/a/"), Value::from("upper")),
            (ArrayKey::str("/b/"), Value::from("length")),
        ]
        .into_iter()
        .collect(),
        [(ArrayKey::Int(0), Value::from("upper"))]
            .into_iter()
            .collect(),
    ] {
        matrix.case(
            "preg_replace_callback_array",
            vec![
                Value::from(patterns),
                keyed.clone(),
                Value::Int(-1),
                reference(),
                Value::Int(768),
            ],
        );
    }
    for subject in [
        list([list([])]),
        list([Value::Int(1), Value::Bool(false), Value::Null]),
    ] {
        matrix.case("preg_grep", vec![Value::from("/./"), subject.clone()]);
        matrix.case(
            "preg_replace",
            vec![
                Value::from("/./"),
                Value::from("X"),
                subject.clone(),
                Value::Int(-1),
                reference(),
            ],
        );
    }
    for callback in [
        Value::Null,
        Value::Int(1),
        Value::Bool(false),
        list([]),
        list([Value::Int(1), Value::Int(2)]),
        list([Value::from("stdClass"), Value::Int(1)]),
    ] {
        matrix.case(
            "preg_replace_callback",
            vec![
                Value::from("/(/"),
                callback.clone(),
                Value::from(""),
                Value::Int(-1),
                reference(),
            ],
        );
        let patterns = [(ArrayKey::str("/(/"), callback)]
            .into_iter()
            .collect::<PhpArray>();
        matrix.case(
            "preg_replace_callback_array",
            vec![
                Value::from(patterns),
                Value::from(""),
                Value::Int(-1),
                reference(),
            ],
        );
    }
}
