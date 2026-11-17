//! Byte-oriented string matrices compared with the installed PHP executable.
use phera_ext_standard::Standard;
use phera_extension_api::{
    ArrayKey, Extension, PhpArray, Registry, Value,
    testing::{TestContext, call, oracle},
};

#[test]
fn strings_match_php() {
    let mut registry = Registry::new();
    Standard.register(&mut registry);
    let mut script = String::from("<?php\n");
    script.push_str(oracle::PHP_PREAMBLE);
    let mut labels = Vec::new();
    let mut results = Vec::new();
    let mut check = |name: &str, mut arguments: Vec<Value>| {
        let expression = format!(
            "{name}({})",
            arguments
                .iter()
                .map(oracle::php_literal)
                .collect::<Vec<_>>()
                .join(",")
        );
        if name == "similar_text" {
            let params = arguments
                .iter()
                .map(oracle::php_literal)
                .collect::<Vec<_>>()
                .join(",");
            script.push_str(&format!("run('{name}',function(){{$p=null;$r=similar_text({params},$p);return [$r,$p];}});\n"));
        } else {
            script.push_str(&format!("run('{name}',fn()=>{expression});\n"));
        }
        labels.push(expression);
        let percent = std::rc::Rc::new(std::cell::RefCell::new(Value::Null));
        if name == "similar_text" {
            arguments.push(Value::Reference(percent.clone()));
        }
        let mut context = TestContext::default();
        let result = call(&registry, &mut context, name, arguments).map(|result| {
            if name == "similar_text" {
                let mut array = PhpArray::new();
                array.insert(ArrayKey::Int(0), result);
                array.insert(ArrayKey::Int(1), percent.borrow().clone());
                Value::from(array)
            } else {
                result
            }
        });
        results.push(oracle::encode_case(
            name,
            &result,
            &context.output,
            &context.diagnostics,
        ));
    };
    let mut strings: Vec<Value> = [
        "",
        "a",
        "A",
        "abc",
        "abcabc",
        "ABcaBc",
        "\0",
        "a\0b",
        "0123456789",
        "\r\n",
        "aaab",
        "xyz",
    ]
    .into_iter()
    .map(Value::from)
    .collect();
    strings.push(Value::from(vec![255, 0, 254, 65, 97]));
    strings.push(Value::from("abCD".repeat(80)));
    let bounds = [
        i64::MIN,
        -1000,
        -10,
        -4,
        -2,
        -1,
        0,
        1,
        2,
        4,
        10,
        1000,
        i64::MAX,
    ];
    for left in &strings {
        for right in &strings {
            for name in ["strcmp", "strcasecmp", "strpbrk"] {
                check(name, vec![left.clone(), right.clone()]);
            }
            for name in ["strstr", "strchr", "stristr", "strrchr"] {
                for before in [false, true] {
                    check(name, vec![left.clone(), right.clone(), Value::Bool(before)]);
                }
            }
            for offset in bounds {
                for name in ["strrpos", "strripos", "strncmp", "strncasecmp"] {
                    check(name, vec![left.clone(), right.clone(), Value::Int(offset)]);
                }
                for length in bounds {
                    for name in ["strspn", "strcspn", "substr_count", "substr_compare"] {
                        check(
                            name,
                            vec![
                                left.clone(),
                                right.clone(),
                                Value::Int(offset),
                                Value::Int(length),
                            ],
                        );
                    }
                }
            }
        }
    }
    for left in &strings {
        for flag in [false, true] {
            check("nl2br", vec![left.clone(), Value::Bool(flag)]);
        }
        for right in &strings {
            for length in [-10, 0, 1, 5, 30] {
                for kind in [-1, 0, 1, 2, 3] {
                    check(
                        "str_pad",
                        vec![
                            left.clone(),
                            Value::Int(length),
                            right.clone(),
                            Value::Int(kind),
                        ],
                    );
                }
            }
            for offset in bounds {
                for length in bounds {
                    check(
                        "substr_replace",
                        vec![
                            left.clone(),
                            right.clone(),
                            Value::Int(offset),
                            Value::Int(length),
                        ],
                    );
                }
            }
            for third in &strings {
                check("strtr", vec![left.clone(), right.clone(), third.clone()]);
            }
        }
    }
    let list = |values: Vec<Value>| {
        let mut array = PhpArray::new();
        for value in values {
            assert!(array.push(value).is_ok());
        }
        Value::from(array)
    };
    let arrays = vec![
        list(vec![]),
        list(strings.clone()),
        list(vec![Value::from("x"), Value::Int(2), Value::Null]),
    ];
    for input in strings.iter().chain(&arrays) {
        for replacement in strings.iter().chain(&arrays) {
            for offset in [
                Value::Int(-2),
                Value::Int(1),
                arrays[0].clone(),
                list(vec![Value::Int(-1), Value::Int(2)]),
            ] {
                for length in [
                    Value::Null,
                    Value::Int(-1),
                    arrays[0].clone(),
                    list(vec![Value::Int(2), Value::Int(0)]),
                ] {
                    check(
                        "substr_replace",
                        vec![input.clone(), replacement.clone(), offset.clone(), length],
                    );
                }
            }
        }
    }
    for input in &strings {
        for entries in [
            vec![],
            vec![("a", "X")],
            vec![("ab", "x"), ("a", "y"), ("abc", "z")],
            vec![("", "x"), ("a", "b")],
        ] {
            let mut array = PhpArray::new();
            for (key, value) in entries {
                array.insert(ArrayKey::str(key), Value::from(value));
            }
            check("strtr", vec![input.clone(), Value::from(array)]);
        }
    }
    let mut escaped = strings.clone();
    escaped.push(Value::from((0..=255).collect::<Vec<u8>>()));
    for value in [
        r"\",
        r"\0\xFF\777\400\xG\8",
        r"\n\r\t\v\a\b\f",
        r".\+*?[^]($)",
    ] {
        escaped.push(Value::from(value));
    }
    for value in &escaped {
        for name in ["addslashes", "stripslashes", "stripcslashes", "quotemeta"] {
            check(name, vec![value.clone()]);
        }
        for mask in [
            "", "a..z", "z..a", "..a", "a..", "...", "\0..ÿ", "\0..\x7f", "a..b..c", "\\",
        ] {
            check("addcslashes", vec![value.clone(), Value::from(mask)]);
        }
    }
    let mut html = strings.clone();
    for value in [
        "<>&\"'",
        "&amp;&lt;&gt;&quot;&apos;&#39;&#x27;&#0;&#xD800;&#x110000;",
        "&copy;&NotEqualTilde;&AMP;&unknown;",
        "é😀\0\x01\x7f",
        "&#000000000000000000000039;",
    ] {
        html.push(Value::from(value));
    }
    for value in [
        vec![0xc0, 0x80],
        vec![0xe2, 0x82],
        vec![0xed, 0xa0, 0x80],
        vec![0xf4, 0x90, 0x80, 0x80],
    ] {
        html.push(Value::from(value));
    }
    for input in html {
        for flags in (-1..=255).chain([i64::MIN, i64::MAX]) {
            check(
                "htmlspecialchars_decode",
                vec![input.clone(), Value::Int(flags)],
            );
            for double in [false, true] {
                for encoding in ["UTF-8", "ISO-8859-1", "wrong", ""] {
                    check(
                        "htmlspecialchars",
                        vec![
                            input.clone(),
                            Value::Int(flags),
                            Value::from(encoding),
                            Value::Bool(double),
                        ],
                    );
                }
            }
        }
    }
    let mut text = strings.clone();
    for value in [
        "The quick brown fox jumped over the lazy dog.",
        "  a b  c    d ",
        "-a-b 'c' -",
        "one\ntwo\r\nthree",
    ] {
        text.push(Value::from(value));
    }
    for input in text {
        for number in bounds {
            for separator in ["", "\n", "<br>", "a"] {
                check(
                    "chunk_split",
                    vec![input.clone(), Value::Int(number), Value::from(separator)],
                );
                for cut in [false, true] {
                    check(
                        "wordwrap",
                        vec![
                            input.clone(),
                            Value::Int(number),
                            Value::from(separator),
                            Value::Bool(cut),
                        ],
                    );
                }
            }
            check("count_chars", vec![input.clone(), Value::Int(number)]);
            for mask in ["", "0..9", "-", "'", "z..a"] {
                check(
                    "str_word_count",
                    vec![input.clone(), Value::Int(number), Value::from(mask)],
                );
            }
        }
        for mode in [3, 4] {
            check("count_chars", vec![input.clone(), Value::Int(mode)]);
        }
    }
    let mut names = strings.clone();
    for name in [
        "Schwern",
        "Smith",
        "Smythe",
        "Ashcraft",
        "Robert",
        "Rupert",
        "knight",
        "aeon",
        "whale",
        "gnome",
        "dumb",
        "xylophone",
        "\0hello",
        "ax",
        "DGE",
        "GH",
        "GNED",
        "CIA",
        "SCI",
    ] {
        names.push(Value::from(name));
    }
    for left in &names {
        check("soundex", vec![left.clone()]);
        for limit in [-1, 0, 1, 2, 4, 20] {
            check("metaphone", vec![left.clone(), Value::Int(limit)]);
        }
        for right in &names {
            check("similar_text", vec![left.clone(), right.clone()]);
            for costs in [[1, 1, 1], [2, 3, 4], [-1, 2, 0], [i64::MAX, 1, i64::MIN]] {
                check(
                    "levenshtein",
                    vec![
                        left.clone(),
                        right.clone(),
                        Value::Int(costs[0]),
                        Value::Int(costs[1]),
                        Value::Int(costs[2]),
                    ],
                );
            }
        }
    }
    let tags = [
        "",
        "<b>bold</b>",
        "<B title='>'>Hi</B>",
        "a < b > c",
        "<!-- x -->yes",
        "<?php echo '?>'; ?>ok",
        "<?xml version='1'?>ok",
        "x<?xml version='1'>ok",
        "<!DOCTYPE html>ok",
        "<a <b>>x",
        "<br/>",
        "<br />",
        "<b\0>text</b>",
        "<script>x < y</script>",
        "<!-->x-->",
        "<![CDATA[x]]>",
        "<b x='<> '>x</b>",
    ];
    for input in tags {
        for allowed in [
            Value::Null,
            Value::from(""),
            Value::from("<b><br><a>"),
            list(vec![Value::from("B"), Value::from("br")]),
        ] {
            check("strip_tags", vec![Value::from(input), allowed]);
        }
    }
    for input in &strings {
        check("base64_encode", vec![input.clone()]);
        for strict in [false, true] {
            check("base64_decode", vec![input.clone(), Value::Bool(strict)]);
        }
    }
    let alphabet = b"AB+/= \t\r\n\x0b\x0c\0\xff";
    for &a in alphabet {
        for &b in alphabet {
            for &c in alphabet {
                for &d in alphabet {
                    let value = Value::from(vec![a, b, c, d]);
                    for strict in [false, true] {
                        check("base64_decode", vec![value.clone(), Value::Bool(strict)]);
                    }
                }
            }
        }
    }
    let all_bytes = Value::from((0..=255).collect::<Vec<u8>>());
    for encoding in [
        "cp1252",
        "cp1251",
        "iso-8859-15",
        "iso-8859-5",
        "koi8-r",
        "cp866",
        "macroman",
        "big5",
        "big5-hkscs",
        "gb2312",
        "sjis",
        "eucjp",
        "UTF8",
        "UTF-8\0x",
    ] {
        for flags in [0, 3, 4, 8, 11, 128, 139, 155, 171, 187] {
            check(
                "htmlspecialchars",
                vec![
                    all_bytes.clone(),
                    Value::Int(flags),
                    Value::from(encoding),
                    Value::Bool(false),
                ],
            );
        }
    }
    // Diagnostic matrices retain null deprecations, invalid scalar conversions and arrays.
    let bad = vec![
        Value::Null,
        Value::Bool(false),
        Value::Float(1.5),
        Value::from("bad"),
        list(vec![]),
    ];
    let specifications = [
        ("strrpos", 2),
        ("strripos", 2),
        ("strstr", 2),
        ("strchr", 2),
        ("stristr", 2),
        ("strrchr", 2),
        ("strpbrk", 2),
        ("substr_count", 2),
        ("substr_compare", 3),
        ("strcmp", 2),
        ("strcasecmp", 2),
        ("strncmp", 3),
        ("strncasecmp", 3),
        ("strspn", 2),
        ("strcspn", 2),
        ("str_pad", 2),
        ("substr_replace", 3),
        ("strtr", 2),
        ("nl2br", 1),
        ("addslashes", 1),
        ("stripslashes", 1),
        ("addcslashes", 2),
        ("stripcslashes", 1),
        ("quotemeta", 1),
        ("htmlspecialchars", 1),
        ("htmlspecialchars_decode", 1),
        ("wordwrap", 1),
        ("chunk_split", 1),
        ("str_word_count", 1),
        ("count_chars", 1),
        ("levenshtein", 2),
        ("soundex", 1),
        ("metaphone", 1),
        ("strip_tags", 1),
        ("base64_encode", 1),
        ("base64_decode", 1),
    ];
    for (name, count) in specifications {
        for index in 0..count {
            for value in &bad {
                let mut arguments = vec![Value::from("abc"); count];
                arguments[index] = value.clone();
                check(name, arguments);
            }
        }
    }
    for input in ["", "abc"] {
        for replacement in [Value::Null, list(vec![])] {
            for key in ["", "a", "absent"] {
                let mut array = PhpArray::new();
                array.insert(ArrayKey::str(key), replacement.clone());
                check("strtr", vec![Value::from(input), Value::from(array)]);
            }
        }
    }
    // Deterministic generated strings stress state transitions and phoneme contexts.
    let mut seed = 1988u64;
    for _ in 0..3000 {
        let mut data = Vec::new();
        for _ in 0..32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let alphabet = b"<>!?-/'\"()ab xmldoctype\0\n\\";
            data.push(alphabet[(seed >> 32) as usize % alphabet.len()]);
        }
        check(
            "strip_tags",
            vec![Value::from(data.clone()), Value::from("<a><b><xml><x>")],
        );
        check("strip_tags", vec![Value::from(data.clone())]);
        check("metaphone", vec![Value::from(data.clone()), Value::Int(4)]);
        check("str_word_count", vec![Value::from(data), Value::Int(2)]);
    }
    for byte in 0..=255u8 {
        for suffix in [b"A".as_slice(), b"GH", b"Y", b"whale"] {
            let mut input = vec![byte];
            input.extend_from_slice(suffix);
            check("metaphone", vec![Value::from(input.clone())]);
            check("soundex", vec![Value::from(input)]);
        }
    }
    for _ in 0..2000 {
        let mut input = Vec::new();
        for _ in 0..12 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            input.push((seed >> 32) as u8);
        }
        for encoding in ["UTF-8", "big5", "big5-hkscs", "gb2312", "sjis", "eucjp"] {
            for flags in [3, 7, 11, 187] {
                check(
                    "htmlspecialchars",
                    vec![
                        Value::from(input.clone()),
                        Value::Int(flags),
                        Value::from(encoding),
                    ],
                );
            }
        }
    }
    let Some(php) = oracle::run_php("standard-strings", &script) else {
        panic!("PHP oracle required")
    };
    let php: Vec<String> = php.iter().map(|line| sign_of_comparison(line)).collect();
    let results: Vec<String> = results
        .iter()
        .map(|line| sign_of_comparison(line))
        .collect();
    oracle::compare("strings", &labels, &php, &results);
    let mut counts = std::collections::BTreeMap::new();
    for line in &results {
        if let Some((name, _)) = line.split_once('=') {
            *counts.entry(name).or_insert(0usize) += 1;
        }
    }
    for (name, count) in counts {
        println!("{name}: {count}");
    }
}

/// The functions PHP implements with the C library's `memcmp`. PHP returns
/// whatever `memcmp` returns for them, and that number is only specified by
/// its sign: one machine answers -1, another -32, a third a large negative
/// number, depending on the C library and even on the CPU model. PHP
/// documents "less than, equal to, or greater than zero", so that is what
/// the comparison with real PHP checks.
const COMPARED_BY_SIGN: [&str; 7] = [
    "strcmp",
    "strcasecmp",
    "strncmp",
    "strncasecmp",
    "substr_compare",
    "strnatcmp",
    "strnatcasecmp",
];

/// Replaces the integer result of one of those functions by its sign, and
/// leaves every other line (other functions, errors, warnings) untouched.
fn sign_of_comparison(line: &str) -> String {
    for name in COMPARED_BY_SIGN {
        let Some(number) = line
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix("=I:"))
        else {
            continue;
        };
        // The encoded line may carry output or diagnostics after the number.
        let (digits, rest) = match number.find('|') {
            Some(end) => number.split_at(end),
            None => (number, ""),
        };
        if let Ok(value) = digits.parse::<i64>() {
            return format!("{name}=I:{}{rest}", value.signum());
        }
    }
    line.to_owned()
}

#[test]
fn comparison_results_are_reduced_to_their_sign() {
    assert_eq!(sign_of_comparison("strcmp=I:-32"), "strcmp=I:-1");
    assert_eq!(
        sign_of_comparison("substr_compare=I:-269484064"),
        "substr_compare=I:-1"
    );
    assert_eq!(sign_of_comparison("strcmp=I:0"), "strcmp=I:0");
    assert_eq!(
        sign_of_comparison("strncasecmp=I:7|O:"),
        "strncasecmp=I:1|O:"
    );
    // Other functions and error results stay exact.
    assert_eq!(sign_of_comparison("strlen=I:-32"), "strlen=I:-32");
    assert_eq!(
        sign_of_comparison("strcmp=E:TypeError:00"),
        "strcmp=E:TypeError:00"
    );
}
