//! Compare locale IDs and UTS #46 details with PHP's intl module.

use phera_extension_api::{ArrayKey, PhpArray, Value};

use crate::matrix::Matrix;

const CORE: &[&str] = &[
    "en_US", "en_GB", "nl_BE", "nl_NL", "fr_BE", "fr_FR", "de_DE", "es_ES", "it_IT", "pt_BR",
    "ja_JP", "ar_EG", "ru_RU", "zh_CN", "hi_IN",
];

#[test]
fn locale_ids() {
    let mut matrix = Matrix::new();
    let ids = [
        "root",
        "und",
        "EN-us",
        "en_US.UTF-8",
        "i-klingon",
        "cel-gaulish",
        "zh-cmn-Hans-CN",
        "zh-Hant-TW",
        "zh_Hant",
        "sl-rozaj-biske",
        "en__US",
        "en_US_POSIX",
        "x-FOO-BAR",
        "de-DE-u-co-phonebk-ca-gregory",
        "iw_IL",
        "en@collation=phonebook;currency=EUR",
        "no_NO_NY",
        "en-US-x-Foo-Bar",
        "de-DE-a-aaa-x-yyy",
        "C",
        "en\0US",
        "qwerty-fake",
        "!!!",
        "sr_Cyrl",
    ]
    .into_iter()
    .chain(CORE.iter().copied());
    for id in ids {
        for name in [
            "locale_get_primary_language",
            "locale_get_region",
            "locale_get_script",
            "locale_canonicalize",
            "locale_parse",
            "locale_get_all_variants",
            "locale_get_keywords",
            "locale_add_likely_subtags",
            "locale_minimize_subtags",
            "locale_is_right_to_left",
            "Locale::getPrimaryLanguage",
            "Locale::getRegion",
            "Locale::getScript",
            "Locale::canonicalize",
            "Locale::parseLocale",
            "Locale::getAllVariants",
            "Locale::getKeywords",
            "Locale::addLikelySubtags",
            "Locale::minimizeSubtags",
            "Locale::isRightToLeft",
        ] {
            matrix.case(name, &[Value::from(id)]);
        }
    }

    for values in [
        &[("language", "nl"), ("region", "BE")][..],
        &[("language", "zh"), ("script", "Hant"), ("region", "TW")][..],
        &[
            ("language", "en"),
            ("variant2", "POSIX"),
            ("private1", "FOO"),
        ][..],
        &[("grandfathered", "i-klingon")][..],
        &[("region", "US")][..],
        &[][..],
    ] {
        let fields: PhpArray = values
            .iter()
            .map(|&(key, value)| (ArrayKey::str(key), Value::from(value)))
            .collect();
        for function in ["locale_compose", "Locale::composeLocale"] {
            matrix.case(function, &[Value::from(fields.clone())]);
        }
    }
    for (tag, range) in [
        ("en_US", "en"),
        ("en", "en_US"),
        ("*", "en"),
        ("ja_JP", "*"),
        ("en_US@currency=EUR", "en-US"),
        ("zh-hant", "zh-Hant-TW"),
        ("", "en"),
    ] {
        for canonical in [false, true] {
            for function in ["locale_filter_matches", "Locale::filterMatches"] {
                matrix.case(
                    function,
                    &[Value::from(tag), Value::from(range), Value::Bool(canonical)],
                );
            }
        }
    }
    let tags: PhpArray = ["en", "en_US", "fr"]
        .into_iter()
        .enumerate()
        .map(|(index, tag)| (ArrayKey::Int(index as i64), Value::from(tag)))
        .collect();
    for range in ["en-US", "en-GB", "en-US-x-test", "fr", "de", "root"] {
        for canonical in [false, true] {
            for function in ["locale_lookup", "Locale::lookup"] {
                matrix.case(
                    function,
                    &[
                        Value::from(tags.clone()),
                        Value::from(range),
                        Value::Bool(canonical),
                        Value::from("default"),
                    ],
                );
            }
        }
    }
    for header in [
        "en-US,fr;q=0.8",
        "*",
        "fr-XX,en;q=0.4",
        "en-XY",
        "fr-CA;q=0",
        "fr;q=0,en;q=.2",
        "",
        "de;q=1.5",
        "fr;q=oops",
        "qwerty",
        "nl-BE,en-US;q=0.7",
    ] {
        for function in ["locale_accept_from_http", "Locale::acceptFromHttp"] {
            matrix.case(function, &[Value::from(header)]);
        }
    }
    matrix.compare("intl-locale-ids");
}

#[test]
fn core_locale_display_names() {
    let mut matrix = Matrix::new();
    for id in CORE {
        for display in CORE {
            for function in [
                "Locale::getDisplayName",
                "locale_get_display_language",
                "locale_get_display_region",
            ] {
                matrix.case(function, &[Value::from(*id), Value::from(*display)]);
            }
        }
    }
    matrix.compare("intl-core-locale-display");
}

#[test]
fn idn_uts46() {
    let mut matrix = Matrix::new();
    let domains = [
        "",
        "www.fußball.com",
        "www.xn--fuball-cta.com",
        "täst.de",
        "a‍b.com",
        "اab.com",
        "abc.مصر",
        "123.مصر",
        "مثال.إختبار",
        "Å。DE",
        "😀.de",
        "xn--",
        "xn--!",
        "xn--a",
        "xn--abc-",
        "xn--tst-qla-",
        "xn--a.b",
        "ab--cd",
        "www.fußball.com-",
        "\u{ad}",
        "a..b",
        ".",
        "..",
        "example.com.",
        "a\0b.de",
        "é_.com",
        "bad_",
        "-é-",
        "́a.com",
    ]
    .into_iter()
    .map(Value::from)
    .chain([
        Value::from(b"\xff.com".to_vec()),
        Value::from(format!("{}.com", "a".repeat(64))),
        Value::from(format!("{}a", "a.".repeat(127))),
    ]);
    for domain in domains {
        for flags in [0, 2, 4, 8, 16, 32, 48, 62, i64::MAX] {
            for function in ["idn_to_ascii", "idn_to_utf8"] {
                matrix.record(
                    function,
                    &[domain.clone(), Value::Int(flags), Value::Int(1)],
                    Some(Value::from("previous")),
                );
            }
        }
    }
    for function in ["idn_to_ascii", "idn_to_utf8"] {
        matrix.case(function, &[Value::from("täst.de")]);
        matrix.case(
            function,
            &[Value::from("domain"), Value::Int(0), Value::Int(10)],
        );
    }
    matrix.compare("intl-idn-uts46");
}
