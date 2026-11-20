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
        ("NFD; [:Nonspacing Mark:] Remove; NFC", "crème brûlée"),
        (
            "NFKC; [:Nonspacing Mark:] Remove; NFKC; Any-Latin; Latin-ASCII;",
            "crème brûlée",
        ),
        ("[:Mn:] Any-Remove", "e\u{301} का \u{1d185}"),
        ("[:^Letter:] Remove", "Café 42"),
        ("Remove", "😀𝟫"),
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
        "こんにちは世界",
        "東京と大阪",
        "hello中文测试ok",
        "A。中、B",
        "五5六",
    ] {
        for id in ["Any-Latin", "Any-Latin;Latin-ASCII;Lower()"] {
            matrix.case(
                "transliterator_transliterate",
                &[Value::from(id), Value::from(text)],
            );
        }
    }
    matrix.case("Transliterator::create", &[Value::from("Any-Latin/BGN")]);
    for id in ["NFD; ", "NFD;;NFC", "NFD;;;"] {
        matrix.case("Transliterator::create", &[Value::from(id)]);
    }
    let invalid = Value::from(vec![255]);
    let locale = [Value::from("en_US")];
    for (left, right) in [
        (invalid.clone(), Value::from("x")),
        (Value::from("x"), invalid.clone()),
    ] {
        matrix.instance("Collator", &locale, "compare", &[left, right], None);
    }
    let mixed = map(&[
        ("word", Value::from("z")),
        ("number", Value::Int(5)),
        ("null", Value::Null),
        ("letter", Value::from("é")),
    ]);
    matrix.instance("Collator", &locale, "sortWithSortKeys", &[], Some(mixed));
    for method in ["sort", "asort", "sortWithSortKeys"] {
        let values = [
            (ArrayKey::Int(0), Value::from("valid")),
            (ArrayKey::Int(1), invalid.clone()),
        ]
        .into_iter()
        .collect::<PhpArray>();
        matrix.instance("Collator", &locale, method, &[], Some(Value::from(values)));
    }
    matrix.compare("intl-collation-and-transforms");
}

#[test]
fn unicode_character_and_property_names() {
    let mut matrix = Matrix::new();
    for code in [
        0, 65, 0x1a2, 0x378, 0x2603, 0x4e00, 0xac00, 0xd800, 0x17000, 0x10ffff, -1,
    ] {
        for kind in [0, 1, 2, 3, 4] {
            matrix.case("IntlChar::charName", &[Value::Int(code), Value::Int(kind)]);
        }
    }
    for (name, kind) in [
        ("SNOWMAN", 0),
        ("snow-man", 0),
        ("latin_capital-letter A", 0),
        ("CJK unified ideograph-04E00", 0),
        ("HANGUL SYLLABLE GA", 0),
        ("LATIN CAPITAL LETTER GHA", 3),
        ("<control-0>", 2),
        ("<unassigned-0378>", 2),
        ("<unassigned-2603>", 2),
        ("snowman\0extra", 0),
        ("", 0),
        ("not a character name", 0),
        ("SNOWMAN", 4),
    ] {
        matrix.case(
            "IntlChar::charFromName",
            &[Value::from(name), Value::Int(kind)],
        );
    }
    for (property, value) in [(0, 1), (3, 0), (4098, 230), (4101, 1), (4106, 25), (-1, 0)] {
        for kind in [0, 1, 2, 99] {
            matrix.case(
                "IntlChar::getPropertyName",
                &[Value::Int(property), Value::Int(kind)],
            );
            matrix.case(
                "IntlChar::getPropertyValueName",
                &[Value::Int(property), Value::Int(value), Value::Int(kind)],
            );
        }
    }
    matrix.compare("intl-unicode-character-and-property-names");
}

#[test]
fn unicode_and_single_byte_conversion() {
    let mut matrix = Matrix::new();
    for encoding in [
        "UTF-8",
        "UTF-16",
        "UTF-16BE",
        "UTF-16LE",
        "UTF-32",
        "UTF-32BE",
        "UTF-32LE",
        "US-ASCII",
        "ISO-8859-1",
        "ISO-8859-5",
        "ISO-8859-15",
        "windows-1250",
        "windows-1251",
        "windows-1252",
        "windows-1256",
        "windows-1258",
    ] {
        for text in ["", "é😀", "cost: €", "Αθήνα", "Београд", "القاهرة", "A\0b"]
        {
            matrix.case(
                "UConverter::transcode",
                &[
                    Value::from(text),
                    Value::from(encoding),
                    Value::from("UTF-8"),
                ],
            );
        }
    }
    for (encoding, bytes) in [
        ("ASCII", &[0xff, b'x'][..]),
        ("UTF-8", &[0xe2, 0x82]),
        ("UTF-16BE", &[0, 0xe9, 0xd8, 0x3d, 0xde, 0]),
        ("UTF-32LE", &[0xe9, 0, 0, 0, 0, 0xf6, 1, 0]),
        ("windows-1252", &[0x80, 0x81, 0xe9]),
        ("ISO-8859-7", &[0xa0, 0xff]),
        ("missing-encoding", b"hello"),
    ] {
        matrix.case(
            "UConverter::transcode",
            &[
                Value::from(bytes.to_vec()),
                Value::from("UTF-8"),
                Value::from(encoding),
            ],
        );
    }
    for substitute in ["?", "", "too long"] {
        matrix.case(
            "UConverter::transcode",
            &[
                Value::from("é😀"),
                Value::from("ASCII"),
                Value::from("UTF-8"),
                map(&[("to_subst", Value::from(substitute))]),
            ],
        );
    }
    let constructor = [Value::from("UTF-16BE"), Value::from("UTF-8")];
    for method in [
        "getSourceEncoding",
        "getDestinationEncoding",
        "getSourceType",
        "getDestinationType",
        "getErrorCode",
        "getErrorMessage",
        "getSubstChars",
    ] {
        matrix.instance("UConverter", &constructor, method, &[], None);
    }
    matrix.instance(
        "UConverter",
        &constructor,
        "convert",
        &[Value::from("é😀")],
        None,
    );
    matrix.instance(
        "UConverter",
        &constructor,
        "convert",
        &[
            Value::from(vec![0, 0xe9, 0xd8, 0x3d, 0xde, 0]),
            Value::Bool(true),
        ],
        None,
    );
    for label in ["UTF-8", "ASCII", "windows-1252", "unknown"] {
        matrix.case("UConverter::getAliases", &[Value::from(label)]);
    }
    matrix.case("UConverter::getStandards", &[]);
    for reason in [-1, 0, 1, 2, 3, 4, 5, 6] {
        matrix.case("UConverter::reasonText", &[Value::Int(reason)]);
    }
    matrix.compare("intl-unicode-and-single-byte-conversion");
}

#[test]
fn calendar_component_errors() {
    let mut matrix = Matrix::new();
    let constructor = [Value::from("UTC"), Value::from("en_US")];
    for (setter, factory, count) in [
        ("setDate", "IntlGregorianCalendar::createFromDate", 3),
        (
            "setDateTime",
            "IntlGregorianCalendar::createFromDateTime",
            6,
        ),
    ] {
        for index in 0..count {
            for invalid in [i64::from(i32::MAX) + 1, i64::from(i32::MIN) - 1] {
                let mut args = vec![Value::Int(1); count];
                args[index] = Value::Int(invalid);
                matrix.instance("IntlGregorianCalendar", &constructor, setter, &args, None);
                matrix.case(factory, &args);
            }
        }
    }
    for (method, value) in [
        ("clear", -1),
        ("setMinimalDaysInFirstWeek", 0),
        ("setSkippedWallTimeOption", 3),
        ("setRepeatedWallTimeOption", 2),
    ] {
        matrix.instance(
            "IntlGregorianCalendar",
            &constructor,
            method,
            &[Value::Int(value)],
            None,
        );
    }
    matrix.compare("intl-calendar-component-errors");
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
        (
            "{stamp, time, yyyy-MM-dd G HH:mm:ss.SSS}",
            map(&[("stamp", Value::Float(1336310569.123))]),
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
        (
            "en_US",
            "On {0,time,yyyy-MM-dd G HH:mm:ss.SSS zzz} then",
            "On 2012-05-06 AD 15:22:49.123 GMT+02:00 then",
        ),
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
    for (name, region) in [
        ("India Standard Time", Value::Null),
        ("Romance Standard Time", Value::from("BE")),
        ("Pacific Standard Time", Value::from("ZZ")),
    ] {
        matrix.case("intltz_get_id_for_windows_id", &[Value::from(name), region]);
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
