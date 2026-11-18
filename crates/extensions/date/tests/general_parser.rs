//! Both public entry points compare directly against real PHP with fixed bases and zones.
mod parser;
mod support;
use phera_extension_api::Value;
use support::Matrix;

const BASES: [i64; 8] = [
    0, 1706704496, 1709206496, 1711845000, 1729992600, 1735689599, 1704628800, 1704715200,
];
const ZONES: [&str; 5] = [
    "UTC",
    "Europe/Brussels",
    "America/New_York",
    "Australia/Lord_Howe",
    "Asia/Kolkata",
];

#[test]
fn parser_examples() {
    let mut matrix = Matrix::new();
    for input in parser::examples() {
        matrix.case("date_parse", vec![Value::from(input.clone())]);
        for base in BASES {
            matrix.case(
                "strtotime",
                vec![Value::from(input.clone()), Value::Int(base)],
            );
        }
    }
    matrix.finish("date-parser-examples");
}

#[test]
fn parser_generated_matrix() {
    let inputs = parser::generated();
    assert!(inputs.len() >= 5000);
    let mut matrix = Matrix::new();
    for input in &inputs {
        matrix.case("date_parse", vec![Value::from(input.clone())]);
    }
    matrix.finish("date-parser-generated-fields");
    for zone in ZONES {
        let mut matrix = Matrix::new();
        matrix.case("date_default_timezone_set", vec![Value::from(zone)]);
        for input in &inputs {
            for base in BASES {
                matrix.case(
                    "strtotime",
                    vec![Value::from(input.clone()), Value::Int(base)],
                );
            }
        }
        matrix.finish(&format!("date-parser-generated-{}", zone.replace('/', "-")));
    }
}

#[test]
fn parser_hostile_bytes() {
    let mut matrix = Matrix::new();
    for byte in 0..=255u8 {
        for tail in [b"".as_slice(), b"2000-01-01", b" 12:00", b" day"] {
            let mut input = vec![byte];
            input.extend_from_slice(tail);
            matrix.case("date_parse", vec![Value::from(input.clone())]);
            matrix.case(
                "strtotime",
                vec![Value::from(input), Value::Int(1706704496)],
            );
        }
    }
    for input in [
        "x".repeat(10000),
        "!".repeat(10000),
        "9".repeat(10000),
        " ".repeat(10000),
        "+1 day ".repeat(10000),
        format!("12:00:00.{}", "9".repeat(10000)),
    ] {
        matrix.case("date_parse", vec![Value::from(input.clone())]);
        matrix.case(
            "strtotime",
            vec![Value::from(input), Value::Int(1706704496)],
        );
    }
    matrix.finish("date-parser-hostile");
}

#[test]
fn parser_ambiguous_and_corrupted() {
    let mut matrix = Matrix::new();
    for input in parser::adversarial::inputs() {
        matrix.case("date_parse", vec![Value::from(input.clone())]);
        matrix.case(
            "strtotime",
            vec![Value::from(input), Value::Int(1706704496)],
        );
    }
    matrix.finish("date-parser-adversarial");
}

#[test]
fn parser_argument_diagnostics() {
    let mut matrix = Matrix::new();
    for value in [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Int(2024),
        Value::Float(1.5),
        Value::Float(f64::NAN),
        Value::Float(f64::INFINITY),
        Value::from(phera_extension_api::PhpArray::new()),
        Value::from(vec![0xff]),
    ] {
        matrix.case("date_parse", vec![value.clone()]);
        matrix.case("strtotime", vec![value.clone(), Value::Int(1706704496)]);
        if !value.is_null() {
            matrix.case("strtotime", vec![Value::from("2000-01-01"), value]);
        }
    }
    for function in ["date_parse", "strtotime"] {
        matrix.case(function, vec![]);
        matrix.case(
            function,
            vec![Value::from("2000-01-01"), Value::Int(0), Value::Int(1)],
        );
    }
    matrix.finish("date-parser-arguments");
}

#[test]
fn parser_zones_fractions_and_extremes() {
    let mut inputs = Vec::new();
    for name in tzdb_data::TZ_NAMES {
        inputs.push(name.to_string());
        inputs.push(format!("2024-10-27 02:30:00 {name}"));
    }
    for row in include_str!("../data/abbreviations.tsv").lines() {
        let name = row.split('\t').next().unwrap();
        inputs.push(format!("2024-10-27 02:30:00 {name}"));
    }
    for len in [
        1, 5, 6, 7, 12, 15, 16, 17, 18, 19, 100, 308, 309, 320, 321, 322, 323, 324, 1000,
    ] {
        for digit in ['0', '1', '9'] {
            for prefix in ["12:34:56.", "@0.", "@-0."] {
                inputs.push(format!("{prefix}{}", digit.to_string().repeat(len)));
            }
        }
    }
    for year in [
        "+9223372036854775807",
        "-9223372036854775808",
        "+9999999999999",
    ] {
        for suffix in ["", " +1 month", " +1 year", " -12 days", " next Monday"] {
            inputs.push(format!("{year}-01-31{suffix}"));
        }
    }
    let mut matrix = Matrix::new();
    for input in inputs {
        matrix.case("date_parse", vec![Value::from(input.clone())]);
        matrix.case(
            "strtotime",
            vec![Value::from(input), Value::Int(1729992600)],
        );
    }
    matrix.finish("date-parser-zones-fractions");
}

#[test]
fn parser_week_and_ordinal_boundaries() {
    let mut matrix = Matrix::new();
    for year in [0, 1, 69, 1900, 2000, 2024, 2026, 9999] {
        for week in [0, 1, 9, 52, 53, 54, 99] {
            let mut inputs = vec![format!("{year:04}-W{week:02}")];
            for day in 0..=8 {
                inputs.push(format!("{year:04}-W{week:02}-{day}"));
                inputs.push(format!("{year:04}W{week:02}{day}"));
            }
            for input in inputs {
                matrix.case("date_parse", vec![Value::from(input.clone())]);
                matrix.case(
                    "strtotime",
                    vec![Value::from(input), Value::Int(1706704496)],
                );
            }
        }
        for ordinal in [0, 1, 59, 60, 365, 366, 367, 999] {
            for separator in ["", ".", "-"] {
                let input = format!("{year:04}{separator}{ordinal:03}");
                matrix.case("date_parse", vec![Value::from(input.clone())]);
                matrix.case(
                    "strtotime",
                    vec![Value::from(input), Value::Int(1706704496)],
                );
            }
        }
    }
    matrix.finish("date-parser-week-ordinal");
}
