//! Format with the PHP objects across locales and check number and date parsing.

use std::time::{SystemTime, UNIX_EPOCH};
use std::{any::Any, rc::Rc};

use crate::matrix::{Matrix, registry};
use phera_extension_api::testing::{TestContext, call_method, instantiate, oracle};
use phera_extension_api::{Object, Value};
use phera_value::ClassHandle;

const LOCALES: &[(&str, &str)] = &[
    ("en_US", "USD"),
    ("en_GB", "GBP"),
    ("nl_BE", "EUR"),
    ("nl_NL", "EUR"),
    ("fr_BE", "EUR"),
    ("fr_FR", "EUR"),
    ("de_DE", "EUR"),
    ("es_ES", "EUR"),
    ("it_IT", "EUR"),
    ("pt_BR", "BRL"),
    ("ja_JP", "JPY"),
    ("ar_EG", "EGP"),
    ("ru_RU", "RUB"),
    ("zh_CN", "CNY"),
    ("hi_IN", "INR"),
];

#[test]
fn core_number_formats() {
    let mut matrix = Matrix::new();
    for &(locale, code) in LOCALES {
        for style in [1, 2, 3, 4] {
            let constructor = [Value::from(locale), Value::Int(style)];
            for amount in [0.0, 0.256, -12.5, 1234.56, -1234.567, 1_234_567.5] {
                matrix.instance(
                    "NumberFormatter",
                    &constructor,
                    "format",
                    &[Value::Float(amount)],
                    None,
                );
                if style == 2 {
                    matrix.instance(
                        "NumberFormatter",
                        &constructor,
                        "formatCurrency",
                        &[Value::Float(amount), Value::from(code)],
                        None,
                    );
                }
            }
        }
    }
    matrix.compare("intl-core-number-formats");
}

#[test]
fn english_spellout_and_ordinals() {
    let mut matrix = Matrix::new();
    for locale in ["en_US", "en_GB"] {
        for style in [5, 6] {
            let constructor = [Value::from(locale), Value::Int(style)];
            for value in [
                Value::Int(i64::MIN),
                Value::Int(-1234),
                Value::Int(0),
                Value::Int(1),
                Value::Int(11),
                Value::Int(21),
                Value::Int(102),
                Value::Int(1042),
                Value::Int(100_000_000),
                Value::Float(1.25),
                Value::Float(-0.125),
                Value::Float(3.5),
            ] {
                matrix.instance("NumberFormatter", &constructor, "format", &[value], None);
            }
            matrix.instance(
                "NumberFormatter",
                &constructor,
                "getTextAttribute",
                &[Value::Int(6)],
                None,
            );
            matrix.instance(
                "NumberFormatter",
                &constructor,
                "setTextAttribute",
                &[Value::Int(6), Value::from("invalid")],
                None,
            );
        }
        matrix.instance(
            "NumberFormatter",
            &[Value::from(locale), Value::Int(5)],
            "setTextAttribute",
            &[Value::Int(6), Value::from("%spellout-ordinal")],
            None,
        );
    }
    matrix.compare("intl-english-spellout-and-ordinals");
}

#[test]
fn significant_digit_patterns_and_symbol_errors() {
    let mut matrix = Matrix::new();
    let long = "@".repeat(200);
    for pattern in ["@@", "@@##", "@#", long.as_str()] {
        let constructor = [Value::from("en_US"), Value::Int(0), Value::from(pattern)];
        matrix.instance(
            "NumberFormatter",
            &constructor,
            "format",
            &[Value::Float(12_345.123456)],
            None,
        );
        matrix.instance("NumberFormatter", &constructor, "getPattern", &[], None);
        for attribute in [16, 17, 18] {
            matrix.instance(
                "NumberFormatter",
                &constructor,
                "getAttribute",
                &[Value::Int(attribute)],
                None,
            );
        }
    }
    let constructor = [Value::from("en_US"), Value::Int(0)];
    for symbol in [-1, 18, 20, 25, 27, 28, 29, 4_294_901_761] {
        matrix.instance(
            "NumberFormatter",
            &constructor,
            "getSymbol",
            &[Value::Int(symbol)],
            None,
        );
        matrix.instance(
            "NumberFormatter",
            &constructor,
            "setSymbol",
            &[Value::Int(symbol), Value::from(".")],
            None,
        );
    }
    matrix.compare("intl-number-significant-patterns-and-symbol-errors");
}

#[test]
fn core_date_formats() {
    let mut matrix = Matrix::new();
    for &(locale, _) in LOCALES {
        for style in [0, 1, 2, 3, -1] {
            for zone in ["UTC", "Europe/Brussels"] {
                let constructor = [
                    Value::from(locale),
                    Value::Int(style),
                    Value::Int(style),
                    Value::from(zone),
                ];
                for timestamp in [0, 1_724_727_600] {
                    if style == 1 {
                        date_zone_difference(&mut matrix, locale, zone, timestamp);
                    }
                    matrix.instance(
                        "IntlDateFormatter",
                        &constructor,
                        "format",
                        &[Value::Int(timestamp)],
                        None,
                    );
                }
            }
        }
    }
    matrix.compare("intl-core-date-formats");
}

#[test]
fn relative_date_styles() {
    let mut matrix = Matrix::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("current Unix time")
        .as_secs() as i64;
    let today_at_noon = now.div_euclid(86_400) * 86_400 + 43_200;
    for locale in ["en_US", "fr_FR", "ja_JP"] {
        for style in 128..=131 {
            for time in [-1, 3] {
                let constructor = [
                    Value::from(locale),
                    Value::Int(style),
                    Value::Int(time),
                    Value::from("UTC"),
                ];
                for offset in -3..=3 {
                    matrix.instance(
                        "IntlDateFormatter",
                        &constructor,
                        "format",
                        &[Value::Int(today_at_noon + 86_400 * offset)],
                        None,
                    );
                }
                matrix.instance("IntlDateFormatter", &constructor, "getPattern", &[], None);
            }
        }
    }
    matrix.compare("intl-relative-date-styles");
}

#[test]
fn date_pattern_generator_skeletons() {
    let mut matrix = Matrix::new();
    for locale in ["en_US", "de_DE", "fr_FR", "ja_JP"] {
        let constructor = [Value::from(locale)];
        for skeleton in [
            "",
            "y",
            "MM",
            "Hm",
            "HHmm",
            "ms",
            "j",
            "jm",
            "jjmm",
            "Jm",
            "yMd",
            "yyyyMMdd",
            "dMy",
            "yMMMMd",
            "yMMMdjm",
            "yMMMMdjm",
            "EEEEyMMMMdjm",
            "yMdHm",
            "Ejm",
            "YYYYMMMdd",
            "YYYYMMMddjjmm",
        ] {
            matrix.instance(
                "IntlDatePatternGenerator",
                &constructor,
                "getBestPattern",
                &[Value::from(skeleton)],
                None,
            );
        }
        matrix.instance(
            "IntlDatePatternGenerator",
            &constructor,
            "getBestPattern",
            &[Value::from(vec![b'j', b'm', 128])],
            None,
        );
    }
    let long_locale = [Value::from("a".repeat(157))];
    matrix.instance(
        "IntlDatePatternGenerator",
        &long_locale,
        "getBestPattern",
        &[],
        None,
    );
    matrix.case("IntlDatePatternGenerator::create", &long_locale);
    matrix.compare("intl-date-pattern-generator");
}

#[test]
fn locale_length_and_time_zone_errors() {
    let mut matrix = Matrix::new();
    let locale = Value::from("a".repeat(157));
    for (class, factory, method, values) in [
        (
            "NumberFormatter",
            "numfmt_create",
            "getErrorCode",
            vec![locale.clone(), Value::Int(1)],
        ),
        (
            "IntlDateFormatter",
            "datefmt_create",
            "getErrorCode",
            vec![locale.clone(), Value::Int(1), Value::Int(1)],
        ),
        (
            "Collator",
            "collator_create",
            "getErrorCode",
            vec![locale.clone()],
        ),
        (
            "MessageFormatter",
            "msgfmt_create",
            "getErrorCode",
            vec![locale.clone(), Value::from("{0}")],
        ),
        (
            "ResourceBundle",
            "resourcebundle_create",
            "getErrorCode",
            vec![locale.clone(), Value::from("")],
        ),
    ] {
        matrix.instance(class, &values, method, &[], None);
        matrix.case(factory, &values);
    }
    matrix.instance("IntlListFormatter", &[locale], "getErrorCode", &[], None);
    for zone in [Value::from("Not/AZone"), Value::from(vec![255])] {
        let mut values = vec![Value::from("en_US"), Value::Int(2), Value::Int(-1)];
        let setter = [zone.clone()];
        matrix.instance("IntlDateFormatter", &values, "setTimeZone", &setter, None);
        values.push(zone.clone());
        matrix.instance("IntlDateFormatter", &values, "getErrorCode", &[], None);
        matrix.case("datefmt_create", &values);
        matrix.case("intlcal_create_instance", &setter);
        matrix.instance("IntlGregorianCalendar", &setter, "getErrorCode", &[], None);
    }
    let styles = [Value::from("en"), Value::Int(-2), Value::Int(0)];
    matrix.instance("IntlDateFormatter", &styles, "getErrorCode", &[], None);
    matrix.case("datefmt_create", &styles);
    matrix.compare("intl-locale-length-and-time-zone-errors");
}

fn date_zone_difference(matrix: &mut Matrix, locale: &str, zone: &str, timestamp: i64) {
    let (prefix, php, phera) = match (locale, zone, timestamp) {
        ("fr_BE" | "fr_FR", "UTC", 0) => ("1 janvier 1970 à 00:00:00", "UTC", "TU"),
        ("fr_BE" | "fr_FR", "UTC", 1_724_727_600) => ("27 août 2024 à 03:00:00", "UTC", "TU"),
        ("pt_BR", "Europe/Brussels", 0) => ("1 de janeiro de 1970 às 01:00:00", "GMT+1", "+1 GMT"),
        ("pt_BR", "Europe/Brussels", 1_724_727_600) => {
            ("27 de agosto de 2024 às 05:00:00", "GMT+2", "+2 GMT")
        }
        ("ar_EG", "UTC", 0) => ("١ يناير ١٩٧٠ في ١٢:٠٠:٠٠ ص", "UTC", "التوقيت العالمي"),
        ("ar_EG", "UTC", 1_724_727_600) => ("٢٧ أغسطس ٢٠٢٤ في ٣:٠٠:٠٠ ص", "UTC", "التوقيت العالمي"),
        _ => return,
    };
    matrix.icu78_difference(
        &format!("{prefix} {php}"),
        &format!("{prefix} {phera}"),
        None,
    );
}

#[test]
fn format_gregorian_calendar_object() {
    // The VM links class ancestry. Native test objects carry only their class name.
    struct GregorianCalendar;

    impl ClassHandle for GregorianCalendar {
        fn name(&self) -> &str {
            "IntlGregorianCalendar"
        }

        fn is_instance_of(&self, name: &str) -> bool {
            name.eq_ignore_ascii_case(self.name()) || name.eq_ignore_ascii_case("IntlCalendar")
        }

        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    let name = "intl-format-gregorian-calendar-object";
    let script = format!(
        "<?php\n{}\n{}",
        oracle::PHP_PREAMBLE,
        r#"run('0', function () {
            $calendar = new IntlGregorianCalendar('Europe/Brussels', 'nl_BE');
            $calendar->setTime(1724727600000.0);
            return IntlDateFormatter::formatObject($calendar, 'yyyy-MM-dd HH:mm', 'nl_BE');
        });"#
    );
    let registry = registry();
    let mut context = TestContext::default();
    let calendar = Rc::new(Object::new(1, Rc::new(GregorianCalendar)));
    call_method(
        &registry,
        &mut context,
        &calendar,
        "__construct",
        vec![Value::from("Europe/Brussels"), Value::from("nl_BE")],
    )
    .unwrap();
    call_method(
        &registry,
        &mut context,
        &calendar,
        "setTime",
        vec![Value::Float(1_724_727_600_000.0)],
    )
    .unwrap();
    let formatter = instantiate(&registry, "IntlDateFormatter").unwrap();
    let result = call_method(
        &registry,
        &mut context,
        &formatter,
        "formatObject",
        vec![
            Value::Object(calendar),
            Value::from("yyyy-MM-dd HH:mm"),
            Value::from("nl_BE"),
        ],
    );
    let php = oracle::run_php(name, &script).expect("PHP 8.5 with intl required");
    let rust = oracle::encode_case("0", &result, &context.output, &context.diagnostics);
    oracle::compare(name, &[], &php, &[rust]);
}

#[test]
fn parsing_offsets() {
    let mut matrix = Matrix::new();
    let decimal = [Value::from("en_US"), Value::Int(1)];
    for input in [
        "1,234.50 extra",
        "-7.25",
        "1e3",
        "",
        "abc",
        "🚀123.4",
        "1.2e-5",
    ] {
        for offset in [0, 1, 4] {
            for kind in [0, 1, 2, 3, 4, 20] {
                matrix.instance(
                    "NumberFormatter",
                    &decimal,
                    "parse",
                    &[Value::from(input), Value::Int(kind)],
                    Some(Value::Int(offset)),
                );
            }
        }
    }
    let date = [
        Value::from("en_US"),
        Value::Int(-1),
        Value::Int(-1),
        Value::from("UTC"),
        Value::Int(1),
        Value::from("yyyy-MM-dd HH:mm:ss"),
    ];
    for input in [
        "2024-01-01 13:00:00xyz",
        "2024-02-31 13:00:00",
        "xxx",
        "2024-01-01",
        "🚀2024-06-17 16:20:30",
    ] {
        for offset in [0, 1, 4] {
            for method in ["parse", "localtime"] {
                matrix.instance(
                    "IntlDateFormatter",
                    &date,
                    method,
                    &[Value::from(input)],
                    Some(Value::Int(offset)),
                );
            }
        }
    }
    matrix.compare("intl-formatters-parse");
}

#[test]
fn date_parse_offset_below_i32() {
    let case = "IntlDateFormatter rejects parse offsets below INT32_MIN";
    if !oracle::supports_php_since(case, 80_513) {
        return;
    }
    let date = [
        Value::from("en_US"),
        Value::Int(0),
        Value::Int(0),
        Value::from("UTC"),
        Value::Null,
        Value::from("yyyy"),
    ];
    let mut matrix = Matrix::new();
    for method in ["parse", "parseToCalendar", "localtime"] {
        matrix.instance(
            "IntlDateFormatter",
            &date,
            method,
            &[Value::from("12026")],
            Some(Value::Int(-4_294_967_295)),
        );
    }
    matrix.compare("intl-date-parse-offset-below-i32");
}
