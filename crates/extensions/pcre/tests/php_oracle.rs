//! PHP 8.5 differential calls exercise returns, references, errors and callback match shapes.

mod support;
use phera_extension_api::{ArrayKey, PhpArray, Value};
use support::{Matrix, list, patterns, reference, subjects};

#[test]
fn matching_matrix() {
    let mut matrix = Matrix::new();
    let patterns = patterns();
    let mut unique = patterns.clone();
    unique.sort_unstable();
    unique.dedup();
    assert!(unique.len() >= 150);
    println!(
        "Pattern corpus: {} entries, {} distinct",
        patterns.len(),
        unique.len()
    );
    for pattern in patterns {
        for subject in subjects() {
            // Without /u, PHP skips UTF checks for (*UTF). Malformed input is undefined
            // in PCRE2, and the safe API deliberately does not reproduce that path.
            if pattern.contains("(*UTF)") && std::str::from_utf8(&subject).is_err() {
                continue;
            }
            for flags in [0, 256, 512, 768] {
                matrix.case(
                    "preg_match",
                    vec![
                        Value::from(pattern),
                        Value::from(subject.clone()),
                        reference(),
                        Value::Int(flags),
                    ],
                );
                for order in [1, 2] {
                    matrix.case(
                        "preg_match_all",
                        vec![
                            Value::from(pattern),
                            Value::from(subject.clone()),
                            reference(),
                            Value::Int(flags | order),
                        ],
                    );
                }
            }
        }
    }
    matrix.compare("matching matrix");
}

#[test]
fn replacement_split_grep_matrix() {
    let mut matrix = Matrix::new();
    for pattern in patterns() {
        for subject in subjects().into_iter().step_by(3) {
            if pattern.contains("(*UTF)") && std::str::from_utf8(&subject).is_err() {
                continue;
            }
            for limit in [-1, 0, 1, 2, -2] {
                matrix.case(
                    "preg_replace",
                    vec![
                        Value::from(pattern),
                        Value::from("<$1:$0>"),
                        Value::from(subject.clone()),
                        Value::Int(limit),
                        reference(),
                    ],
                );
                matrix.case(
                    "preg_filter",
                    vec![
                        Value::from(pattern),
                        Value::from("x"),
                        Value::from(subject.clone()),
                        Value::Int(limit),
                        reference(),
                    ],
                );
                for flags in [0, 1, 2, 3, 4, 5, 6, 7] {
                    matrix.case(
                        "preg_split",
                        vec![
                            Value::from(pattern),
                            Value::from(subject.clone()),
                            Value::Int(limit),
                            Value::Int(flags),
                        ],
                    );
                }
            }
            for flags in [0, 1, -1, 2] {
                matrix.case(
                    "preg_grep",
                    vec![
                        Value::from(pattern),
                        list([Value::from(subject.clone()), Value::Int(123), Value::Null]),
                        Value::Int(flags),
                    ],
                );
            }
        }
    }
    matrix.compare("replacement split grep matrix");
}

#[test]
fn edges_and_arrays() {
    let mut matrix = Matrix::new();
    for pattern in [
        "/a/",
        "/(a)(b)?/",
        "//u",
        "/(?<x>a)|(?<x>b)/J",
        "/(/",
        "/(*UTF)./",
    ] {
        for subject in [b"ab".as_slice(), "éa".as_bytes(), b"\xffa"] {
            for offset in [i64::MIN, -99, -2, -1, 0, 1, 2, 3, 99, i64::MAX] {
                if pattern.contains("(*UTF)") {
                    let position = if offset < 0 {
                        subject.len().saturating_sub(offset.unsigned_abs() as usize)
                    } else {
                        offset as usize
                    };
                    if std::str::from_utf8(subject).is_err()
                        || subject
                            .get(position)
                            .is_some_and(|byte| byte & 0xc0 == 0x80)
                    {
                        continue;
                    }
                }
                for flags in [0, 1, 2, 3, 255, 256, 512, 768, 1024, -1] {
                    for name in ["preg_match", "preg_match_all"] {
                        matrix.case(
                            name,
                            vec![
                                Value::from(pattern),
                                Value::from(subject.to_vec()),
                                reference(),
                                Value::Int(flags),
                                Value::Int(offset),
                            ],
                        );
                    }
                }
            }
        }
    }
    for replacement in [
        "$0", "$1", "${1}", "\\1", "$12", "$123", "${123}", "${12}", "\\12", "\\123", "\\$1",
        "\\\\1", "$$1", "\\x", "$x", "\\\\$1", "${0}", "$99",
    ] {
        for pattern in ["/(a)(b)?/", "/(a)|(b)/", "//"] {
            matrix.case(
                "preg_replace",
                vec![
                    Value::from(pattern),
                    Value::from(replacement),
                    Value::from("abc"),
                    Value::Int(-1),
                    reference(),
                ],
            );
        }
    }
    let keyed = Value::from(
        [
            (ArrayKey::str("first"), Value::from("abc")),
            (ArrayKey::Int(10), Value::from("b")),
            (ArrayKey::str("bad"), Value::from(vec![0xff])),
        ]
        .into_iter()
        .collect::<PhpArray>(),
    );
    for pattern in [
        Value::from("/a/"),
        list([Value::from("/a/"), Value::from("/b/")]),
        list([]),
        list([Value::from("/a/"), Value::from("/(/"), Value::from("/b/")]),
    ] {
        for replacement in [
            Value::from("b"),
            list([Value::from("b"), Value::from("c")]),
            list([]),
        ] {
            for subject in [Value::from("ab"), keyed.clone(), list([])] {
                for name in ["preg_replace", "preg_filter"] {
                    matrix.case(
                        name,
                        vec![
                            pattern.clone(),
                            replacement.clone(),
                            subject.clone(),
                            Value::Int(-1),
                            reference(),
                        ],
                    );
                }
            }
        }
    }
    for delimiter in [
        Value::Null,
        Value::from("/"),
        Value::from("ab"),
        Value::from(""),
        Value::from("\0"),
    ] {
        matrix.case(
            "preg_quote",
            vec![Value::from((0..=255).collect::<Vec<u8>>()), delimiter],
        );
    }
    for pattern in ["/^a+$/", "/a{99999}/", "/b/", "/a$/", "/(?<x>a+)/"] {
        let subject = Value::from("a".repeat(100_000));
        matrix.case(
            "preg_match",
            vec![Value::from(pattern), subject.clone(), reference()],
        );
        matrix.case(
            "preg_replace",
            vec![
                Value::from(pattern),
                Value::from("b"),
                subject.clone(),
                Value::Int(1),
                reference(),
            ],
        );
        matrix.case(
            "preg_split",
            vec![Value::from(pattern), subject, Value::Int(2)],
        );
    }
    matrix.compare("edges and arrays");
}

#[test]
fn callbacks_matrix() {
    let mut matrix = Matrix::new();
    for pattern in [
        "/a/",
        "/(a)(b)?/",
        "/(?<x>a)|(?<x>b)/J",
        "//",
        "/(?:|a)/",
        "/x*/",
        "/./u",
        "/(/",
    ] {
        for subject in subjects().into_iter().step_by(3) {
            for callback in [
                "upper",
                "reverse",
                "length",
                "inspect",
                "null_result",
                "array_result",
                "fail",
            ] {
                for flags in [0, 256, 512, 768, -1] {
                    matrix.case(
                        "preg_replace_callback",
                        vec![
                            Value::from(pattern),
                            Value::from(callback),
                            Value::from(subject.clone()),
                            Value::Int(-1),
                            reference(),
                            Value::Int(flags),
                        ],
                    );
                    let patterns = [(ArrayKey::str(pattern), Value::from(callback))]
                        .into_iter()
                        .collect::<PhpArray>();
                    matrix.case(
                        "preg_replace_callback_array",
                        vec![
                            Value::from(patterns),
                            Value::from(subject.clone()),
                            Value::Int(2),
                            reference(),
                            Value::Int(flags),
                        ],
                    );
                }
            }
        }
    }
    matrix.compare("callbacks matrix");
}

#[test]
fn catastrophic_pattern_is_bounded() {
    let mut matrix = Matrix::new();
    let start = std::time::Instant::now();
    matrix.case(
        "preg_match",
        vec![
            Value::from("/(a+)+$/"),
            Value::from(format!("{}b", "a".repeat(30))),
            reference(),
        ],
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    for pattern in [
        "/(*LIMIT_MATCH=2000000)(a+)+$/",
        "/(*LIMIT_MATCH=100)(a+)+$/",
        "/(*NO_JIT)(a+)+$/",
        "/(?:|(a+)+$)/",
    ] {
        matrix.case(
            "preg_match",
            vec![
                Value::from(pattern),
                Value::from(format!("{}b", "a".repeat(30))),
            ],
        );
        matrix.case(
            "preg_match",
            vec![
                Value::from(pattern),
                Value::from(format!("{}b", "a".repeat(30))),
                reference(),
            ],
        );
    }
    assert!(start.elapsed() < std::time::Duration::from_secs(1));
    matrix.compare("catastrophic pattern");
}

#[test]
fn binary_scoped_and_callback_edges() {
    let mut matrix = Matrix::new();
    support::edge_cases(&mut matrix);
    matrix.compare("binary scoped callback edges");
}
