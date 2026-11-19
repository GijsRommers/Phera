//! Format with the PHP objects across locales and check number and date parsing.

use crate::matrix::Matrix;
use phera_extension_api::Value;
use phera_extension_api::testing::oracle;

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
