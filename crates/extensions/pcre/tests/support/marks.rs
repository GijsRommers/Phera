//! PHP oracle cases for match arrays that contain a PCRE verb's MARK.

use super::{Harness, Matrix, reference};
use phera_extension_api::testing::oracle;
use phera_extension_api::{ArrayKey, PhpArray, Value};

#[test]
fn mark_matrix() {
    let mut matrix = Matrix::new();
    let cases: &[(&str, &[&str])] = &[
        (
            r"{^(?|/api/products/(\d+)(*:35)|/api/users/([^/]++)/posts(*:60))/?$}sDu",
            &["/api/products/12", "/api/users/ada/posts", "/missing"],
        ),
        ("/(?:a(*:x)|b(*:y))+/", &["ab", "ba", "aba", "bab"]),
        ("/a(*:one)(*:two)b/", &["ab", "xb"]),
        ("/(?=a(*:look))ab/", &["ab", "ba"]),
        ("/(?<=a(*:behind))b/", &["ab", "bb"]),
        ("/(?=(?:a(*:peek)))ac|ab/", &["ab", "ac"]),
        ("/(*pla:a(*:look))ab/", &["ab"]),
        ("/(?>a(*:atomic)|b(*:other))c/", &["ac", "bc", "ad"]),
        ("/a(*PRUNE:p)b|ac/", &["ab", "ac"]),
        ("/a(*THEN:t)b|ac/", &["ab", "ac"]),
        ("/a(*COMMIT:c)b|ac/", &["ab", "ac"]),
        ("/a(*:earlier)b(*PRUNE:later)c/", &["abc"]),
        ("/x(*MARK:m)y/", &["xy", "xz"]),
        ("/a|b(*:bee)|c(*:see)/", &["abac", "aaa", "bacb"]),
        (
            "/(?:(?<name>a)(b)?(*:first)|c(*:second))+/",
            &["abcb", "aca", "z"],
        ),
        ("/(?|(a)(*:first)|(b)(*:second))(c)?/", &["abba", "bc", "d"]),
        (
            "/(?|(a)(*:first)|(?|(b)(*:second)|(c)(*:third)))/",
            &["abc"],
        ),
        ("/a(*:café-猫)/u", &["a", "à"]),
        ("/a(*:a # b)/x", &["a"]),
        ("/a # (*:ignored)\n b(*:real)/x", &["ab", "a"]),
        ("/(*ANY)a # Å (*:ignored)\n b(*:real)/xu", &["ab"]),
        ("/(?xx)(a) # (*:ignored)\n b(*:real)/", &["ab"]),
        ("/(?x:a # (*:ignored)\n b(*:inner))#c(*:outer)/", &["ab#c"]),
        ("/(?x)(?-x:a#b(*:real)) # (*:ignored)\n/", &["a#b"]),
        ("/(?# (*:ignored)a(*:real)/", &["a"]),
        (r"/\Q(*:ignored)\E(a)(*:actual)/", &["(*:ignored)a", "a"]),
        (r"/\(\*:ignored\)(a)(*:actual)/", &["(*:ignored)a"]),
        (r"/[(*:ignored)]+([[:alpha:]]+)(*:actual)/", &["(:a"]),
        (r"/[](*:fake)]+(z)(*:real)/", &["]z"]),
        (r"/[^](*:fake)]+(z)(*:real)/", &["bz"]),
        (r"/[\](*:fake)]+(z)(*:real)/", &["]z"]),
        (r"/[\Q](*:fake)\E]+(z)(*:real)/", &["]z"]),
        (r"/[\Q\E](*:fake)]+(z)(*:real)/", &["]z"]),
        ("/(?xx)[ ](*:fake)]+(z)(*:real)/", &["]z"]),
        ("/(?C\"(*:ignored)\")a(*:real)/", &["a"]),
        ("/(a)(?<name>b)(*:real)/n", &["ab"]),
        ("/(?n:(a)(*:inner))(?-n:(b)(*:outer))/", &["ab"]),
        ("/(?(?=a)(a)(*:yes)|(b)(*:no))/", &["ab"]),
        (r"/a(*:skip)b(*SKIP:skip)c|d(*:real)/", &["abd", "abc"]),
        ("/(?:|a(*:nonempty))/", &["a", "b", "aa"]),
        ("/(?:a(*:a)|(*:empty))/A", &["ab", "ba"]),
        (r"/x\Ka(*:real)/", &["xa", "xaxa"]),
    ];
    for &(pattern, subjects) in cases {
        for &subject in subjects {
            for flags in [0, 256, 512, 768] {
                matrix.case(
                    "preg_match",
                    vec![
                        pattern.into(),
                        subject.into(),
                        reference(),
                        Value::Int(flags),
                    ],
                );
                for order in [0, 1, 2] {
                    matrix.case(
                        "preg_match_all",
                        vec![
                            pattern.into(),
                            subject.into(),
                            reference(),
                            Value::Int(flags | order),
                        ],
                    );
                }
            }
        }
    }
    for flags in [0, 256, 512, 768] {
        for pattern in ["/(?<x>a)(b)?(*:one)|c/", "/(?:|a(*:nonempty))/"] {
            let subject = "acabab";
            matrix.case(
                "preg_replace_callback",
                vec![
                    pattern.into(),
                    "inspect".into(),
                    subject.into(),
                    Value::Int(-1),
                    reference(),
                    Value::Int(flags),
                ],
            );
            let callbacks = [(ArrayKey::str(pattern), "inspect".into())]
                .into_iter()
                .collect::<PhpArray>();
            matrix.case(
                "preg_replace_callback_array",
                vec![
                    Value::from(callbacks),
                    subject.into(),
                    Value::Int(-1),
                    reference(),
                    Value::Int(flags),
                ],
            );
        }
    }
    matrix.case(
        "preg_match",
        vec![
            r"/\Gx\Ka(*:mark)/A".into(),
            "zxa".into(),
            reference(),
            Value::Int(256),
            Value::Int(1),
        ],
    );
    matrix.case(
        "preg_match",
        vec![
            Value::from(b"/a(*:\xff)/".to_vec()),
            "a".into(),
            reference(),
        ],
    );
    matrix.case(
        "preg_match",
        vec![
            Value::from(b"/a(*:ab\0c)/".to_vec()),
            "a".into(),
            reference(),
        ],
    );
    for (function, arguments) in [
        ("preg_match", vec!["/a(*:mark)/".into(), "a".into()]),
        ("preg_split", vec!["/a(*:mark)/".into(), "bab".into()]),
        (
            "preg_replace",
            vec!["/a(*:mark)/".into(), "x".into(), "a".into()],
        ),
        (
            "preg_grep",
            vec!["/a(*:mark)/".into(), super::list(["a".into(), "b".into()])],
        ),
    ] {
        matrix.case(function, arguments);
    }
    matrix.compare("mark matrix");
}

/// Pin both runtimes' full results for limitations of the auxiliary pattern.
#[test]
fn mark_differences() {
    let cases = [
        (r"/(?|(a)(*:1)|(b)(*:2))\1/", "bb", "2", None),
        (r"/a(*:one)(b)\1/", "abb", "one", None),
        ("/(?=ab(*:ahead))a(*:after)b/", "ab", "after", Some("ahead")),
        (
            "/b(*:after)(?<=a(*:behind)b)/",
            "ab",
            "behind",
            Some("after"),
        ),
    ];
    let mut script = format!("<?php\n{}\n", oracle::PHP_PREAMBLE);
    let mut expected = Vec::new();
    let mut harness = Harness::default();
    for (pattern, subject, php_mark, phera_mark) in cases {
        let regex = Value::from(pattern);
        let text = Value::from(subject);
        script.push_str(&format!(
            "preg_match({}, {}, $m); echo enc($m), \"\\n\";\n",
            oracle::php_literal(&regex),
            oracle::php_literal(&text),
        ));
        let matched = reference();
        let count = harness
            .run("preg_match", vec![regex, text, matched.clone()])
            .unwrap();
        assert_eq!(oracle::encode(&count), oracle::encode(&Value::Int(1)));
        let Value::Array(actual) = &*matched.deref() else {
            panic!("preg_match must return a match array");
        };
        let key = ArrayKey::str("MARK");
        assert_eq!(
            actual.get(&key).map(oracle::encode),
            phera_mark.map(|mark| oracle::encode(&Value::from(mark))),
            "{pattern}",
        );
        let mut php_matches = actual.as_ref().clone();
        php_matches.insert(key, php_mark.into());
        expected.push(oracle::encode(&Value::from(php_matches)));
        println!("{pattern}: PHP MARK {php_mark:?}, Phera MARK {phera_mark:?}");
    }
    let php = oracle::run_php("pcre-mark-differences", &script).expect("PHP 8.5 oracle required");
    assert_eq!(php, expected);
}
