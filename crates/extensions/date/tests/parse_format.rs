//! Format parsing checks successful fields and malformed-input diagnostic arrays.
mod support;
use phera_extension_api::Value;
use support::Matrix;

#[test]
fn format_grammar_matrix() {
    let mut matrix = Matrix::new();
    let mut inputs: Vec<String> = [
        "",
        "bad",
        "2024-02-29",
        "2024-02-30",
        "2023-02-29",
        "12:34:56.123456",
        "12:34:56.7",
        "12:3:4",
        "25:61:62",
        "January",
        "Sept",
        "December",
        "Monday",
        "Tue",
        "weekdays",
        "next",
        "am",
        "pm",
        "a.m.",
        "p.m.",
        "12 pm",
        "9 a.m.",
        "2024 365",
        "-1",
        "+10000",
        "--2024",
        "+-2024",
        "abc12",
        "UTC",
        "utc",
        "GMT",
        "EDT",
        "CET",
        "Z",
        "Europe/Brussels",
        "Europe/brussels",
        "+02:30",
        "-0730",
        "+2",
        "+24:99",
        "GMT+02:30",
        "(CEST)",
        "Foo/Bar",
        "2024-01-01stuff",
        "2024-x-1",
        "2024/02/03",
        "\t",
        " ",
        "\u{a0}",
        "\u{202f}",
        "12th",
        "2nd",
        "00",
        "99",
        "1234567",
        "9223372036854775807",
        "9223372036854775808",
        "-9223372036854775808",
        "-9223372036854775809",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    for number in 0..100 {
        inputs.push(number.to_string());
    }
    for format in [
        "Y",
        "y",
        "X",
        "x",
        "m",
        "n",
        "d",
        "j",
        "z",
        "H",
        "G",
        "g",
        "h",
        "i",
        "s",
        "u",
        "v",
        "U",
        "D",
        "l",
        "M",
        "F",
        "a",
        "A",
        "S",
        "e",
        "O",
        "P",
        "p",
        "T",
        "?",
        "*",
        "#",
        "!",
        "|",
        "+",
        "\\",
        "\\Y",
        "Y-m-d",
        "Y#m#d",
        "!Y-m-d",
        "Y-m-d|",
        "Y-m-d+",
        "!Y-m-d H:i:s.u",
        "H:i:s",
        "h a",
        "Y z",
        "D, d M Y H:i:s O",
        "U.u",
        "!e!",
        "o-W-N",
        "\\!Y",
        "Y\0ignored",
    ] {
        for input in &inputs {
            matrix.case(
                "date_parse_from_format",
                vec![Value::from(format), Value::from(input.clone())],
            );
        }
    }
    for input in [
        Value::from(b"2024\0-01-01".to_vec()),
        Value::from(vec![255]),
        Value::Null,
        Value::Bool(false),
        Value::Int(2024),
        Value::Float(1.5),
    ] {
        matrix.case("date_parse_from_format", vec![Value::from("Y-m-d"), input]);
    }
    matrix.finish("date-parse-format-grammar");
}

#[test]
fn complete_dates_and_timezones() {
    let mut matrix = Matrix::new();
    for year in [0, 1, 69, 99, 100, 1800, 1900, 2000, 2024, 2100, 2400, 9999] {
        for month in 0..=13 {
            for day in [0, 1, 28, 29, 30, 31, 32] {
                for suffix in ["", " 23:59:59.123456", " 24:60:60.9", " 12:00:00.000000"] {
                    let input = format!("{year:04}-{month:02}-{day:02}{suffix}");
                    let format = if suffix.is_empty() {
                        "!Y-m-d"
                    } else {
                        "!Y-m-d H:i:s.u"
                    };
                    matrix.case(
                        "date_parse_from_format",
                        vec![Value::from(format), Value::from(input)],
                    );
                }
            }
        }
    }
    for name in tzdb_data::TZ_NAMES {
        for prefix in ["e", "!e", "!e!", "!e|", "e+"] {
            matrix.case(
                "date_parse_from_format",
                vec![Value::from(prefix), Value::from(*name)],
            );
        }
    }
    for abbreviation in include_str!("../data/abbreviations.tsv").lines() {
        matrix.case(
            "date_parse_from_format",
            vec![
                Value::from("e"),
                Value::from(abbreviation.split('\t').next().unwrap()),
            ],
        );
    }
    matrix.finish("date-parse-format-dates");
}

#[test]
fn byte_inputs_and_relative_unit_words() {
    let mut matrix = Matrix::new();
    for letter in b"YyXxmdjzHGghisuUvDlMFaASPeO?*#!|+\\" {
        for byte in 1..=255u8 {
            for tail in [b"".as_slice(), b"123456789", b"2000-01-01"] {
                let mut input = vec![byte];
                input.extend_from_slice(tail);
                matrix.case(
                    "date_parse_from_format",
                    vec![Value::from(vec![*letter]), Value::from(input)],
                );
            }
        }
    }
    for word in [
        "ms",
        "msec",
        "millisecond",
        "µs",
        "usec",
        "µsec",
        "microseconds",
        "forthnight",
        "forthnights",
        "mondays",
        "tuesdays",
        "wednesdays",
        "thursdays",
        "fridays",
        "saturdays",
        "sundays",
        "weekdays",
    ] {
        matrix.case(
            "date_parse_from_format",
            vec![Value::from("D"), Value::from(word)],
        );
    }
    matrix.finish("date-parse-format-bytes");
}
