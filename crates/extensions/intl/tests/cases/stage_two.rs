//! Verify the additional intl APIs directly against native PHP 8.5.

use phera_extension_api::{ArrayKey, PhpArray, Value};

use crate::Matrix;

fn map(entries: &[(&str, Value)]) -> Value {
    let values: PhpArray = entries
        .iter()
        .map(|(key, value)| (ArrayKey::str(*key), value.clone()))
        .collect();
    Value::from(values)
}

#[test]
fn collation_and_transforms() {
    let mut matrix = Matrix::new();
    for locale in [
        "en_US", "nl_BE", "fr_FR", "de_DE", "es_ES", "ja_JP", "ar_EG", "ru_RU",
    ] {
        for (left, right) in [
            ("é", "e\u{301}"),
            ("A", "a"),
            ("a", "ä"),
            ("2", "10"),
            ("東京", "大阪"),
        ] {
            matrix.instance(
                "Collator",
                &[Value::from(locale)],
                "compare",
                &[Value::from(left), Value::from(right)],
                None,
            );
        }
    }
    for (id, text) in [
        ("Latin-ASCII", "Crème brûlée à Bruxelles"),
        ("Any-Latin; Latin-ASCII; Lower()", "À New York"),
        ("Upper", "Straße"),
        ("NFD", "é"),
        ("Null", "😀𝟫"),
    ] {
        matrix.case(
            "transliterator_transliterate",
            &[Value::from(id), Value::from(text)],
        );
    }
    for text in [
        "Crème brûlée à Bruxelles",
        "Ελληνική γλώσσα",
        "Санкт-Петербург",
        "اللغة العربية",
        "नमस्ते दुनिया",
        "中文测试",
    ] {
        for id in ["Any-Latin", "Any-Latin;Latin-ASCII;Lower()"] {
            matrix.case(
                "transliterator_transliterate",
                &[Value::from(id), Value::from(text)],
            );
        }
    }
    matrix.case("Transliterator::create", &[Value::from("Any-Latin/BGN")]);
    matrix.compare("intl-collation-and-transforms");
}

#[test]
fn message_format_arguments() {
    let mut matrix = Matrix::new();
    let inputs = [
        (
            "{name} has {count, number, integer} files",
            map(&[("name", Value::from("Anne")), ("count", Value::Int(1234))]),
        ),
        (
            "{kind, select, cat {Cat} dog {Dog} other {Pet}}",
            map(&[("kind", Value::from("dog"))]),
        ),
        (
            "{n, plural, =0 {none} one {# item} other {# items}}",
            map(&[("n", Value::Int(2))]),
        ),
        (
            "{n, plural, offset:1 =1 {only you} other {you and # others}}",
            map(&[("n", Value::Int(124))]),
        ),
        (
            "{n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}",
            map(&[("n", Value::Int(22))]),
        ),
        ("''{missing}''", map(&[])),
        (
            "{ratio, number, percent}",
            map(&[("ratio", Value::Float(0.25))]),
        ),
        (
            "{stamp, date, short} {stamp, time, short}",
            map(&[("stamp", Value::Int(1724727600))]),
        ),
    ];
    for locale in ["en_US", "fr_FR", "nl_BE", "ar_EG"] {
        for (pattern, arguments) in &inputs {
            let constructor = [Value::from(locale), Value::from(*pattern)];
            matrix.instance(
                "MessageFormatter",
                &constructor,
                "format",
                std::slice::from_ref(arguments),
                None,
            );
            matrix.case(
                "msgfmt_format_message",
                &[
                    constructor[0].clone(),
                    constructor[1].clone(),
                    arguments.clone(),
                ],
            );
        }
    }
    for (locale, pattern, value) in [
        ("en_US", "{0,number}", "1,234"),
        ("en_US", "{0,number,integer}", "-1,234.56"),
        ("en_US", "{0,number,currency}", "$12.34"),
        ("en_US", "{0,number,percent}", "23%"),
        ("en_US", "{0,number}", "123abc"),
        ("fr_FR", "{0,number}", "1\u{202f}234,5"),
        ("nl_BE", "{0,number,percent}", "23%"),
    ] {
        matrix.case(
            "msgfmt_parse_message",
            &[
                Value::from(locale),
                Value::from(pattern),
                Value::from(value),
            ],
        );
    }
    matrix.compare("intl-message-format");
}

#[test]
fn zone_metadata_and_character_operations() {
    let mut matrix = Matrix::new();
    for name in [
        "Europe/Brussels",
        "America/New_York",
        "UTC",
        "US/Eastern",
        "Asia/Calcutta",
    ] {
        for function in [
            "intltz_get_region",
            "intltz_get_iana_id",
            "intltz_get_windows_id",
        ] {
            matrix.case(function, &[Value::from(name)]);
        }
        matrix.record(
            "intltz_get_canonical_id",
            &[Value::from(name)],
            Some(Value::Null),
        );
    }
    for chr in [
        Value::from("é"),
        Value::from("Ω"),
        Value::Int(65),
        Value::Int(0x10428),
        Value::Int(0x1f60a),
    ] {
        for method in [
            "ord",
            "chr",
            "toupper",
            "tolower",
            "charType",
            "getCombiningClass",
            "isalpha",
            "isdigit",
        ] {
            matrix.case(&format!("IntlChar::{method}"), std::slice::from_ref(&chr));
        }
    }
    for value in ["1", "٧", "１", "𝟡", "A"] {
        matrix.case("IntlChar::charDigitValue", &[Value::from(value)]);
        matrix.case("IntlChar::digit", &[Value::from(value), Value::Int(16)]);
    }
    matrix.compare("intl-zone-and-character");
}
