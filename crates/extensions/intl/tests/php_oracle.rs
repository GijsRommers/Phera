//! Compare PHP values, changed references and diagnostics with PHP's intl module.

use std::cell::RefCell;
use std::fmt::Write;
use std::rc::Rc;

use phera_ext_intl::Intl;
use phera_extension_api::testing::{TestContext, call, call_method, instantiate, oracle};
use phera_extension_api::{ArrayKey, ClassEntry, PhpArray, Registry, Value};

struct Matrix {
    registry: Registry,
    script: String,
    labels: Vec<String>,
    results: Vec<String>,
}

impl Matrix {
    fn new() -> Self {
        let mut registry = Registry::new();
        registry.class(ClassEntry::class("Exception"));

        registry.load(&Intl);
        let script = format!("<?php\n{}\n", oracle::PHP_PREAMBLE);
        Self {
            registry,
            script,
            labels: Vec::new(),
            results: Vec::new(),
        }
    }

    fn case(&mut self, function: &str, arguments: &[Value]) {
        self.record(function, arguments, None);
    }

    fn record(&mut self, function: &str, arguments: &[Value], next: Option<Value>) {
        let tag = self.labels.len().to_string();
        let mut php_arguments: Vec<_> = arguments.iter().map(oracle::php_literal).collect();
        let setup = next.as_ref().map_or(String::new(), |next| {
            php_arguments.push("$next".to_owned());
            format!("$next = {};", oracle::php_literal(next))
        });
        let call_text = format!("{function}({})", php_arguments.join(", "));
        let changed = if next.is_some() { ", $next" } else { "" };
        let reads_error = !function.starts_with("intl_");
        let get_error = if reads_error {
            ", intl_get_error_code(), intl_get_error_message()"
        } else {
            ""
        };
        writeln!(
            self.script,
            "normalizer_normalize(''); run('{tag}', function() {{ {setup} \
             return [{call_text}{get_error}{changed}]; }});"
        )
        .unwrap();
        self.labels.push(call_text);

        let mut context = TestContext::default();
        let mut arguments = arguments.to_vec();
        let next = next.map(|initial| Rc::new(RefCell::new(initial)));
        if let Some(next) = &next {
            arguments.push(Value::Reference(Rc::clone(next)));
        }
        let result = if let Some((class, method)) = function.split_once("::") {
            let object = instantiate(&self.registry, class).unwrap();
            call_method(&self.registry, &mut context, &object, method, arguments)
        } else {
            call(&self.registry, &mut context, function, arguments)
        };
        let result = result.map(|value| {
            let mut values = vec![value];
            if reads_error {
                for getter in ["intl_get_error_code", "intl_get_error_message"] {
                    values.push(call(&self.registry, &mut context, getter, vec![]).unwrap());
                }
            }
            if let Some(next) = next {
                values.push(next.borrow().clone());
            }
            let array: PhpArray = values
                .into_iter()
                .enumerate()
                .map(|(index, value)| (ArrayKey::Int(index as i64), value))
                .collect();
            Value::from(array)
        });
        self.results.push(oracle::encode_case(
            &tag,
            &result,
            &context.output,
            &context.diagnostics,
        ));
    }

    fn compare(&self, name: &str) {
        let php = oracle::run_php(name, &self.script).expect("PHP 8.5 with intl required");
        oracle::compare(name, &self.labels, &php, &self.results);
    }
}

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
