//! Cross every PHP format character with calendar, ISO-week and timezone boundaries.
mod support;
use phera_ext_date::calendar::Civil;
use phera_extension_api::Value;
use support::Matrix;

#[test]
fn all_format_letters_and_calendar_arrays() {
    let zones = [
        "UTC",
        "Europe/Brussels",
        "Europe/London",
        "Europe/Dublin",
        "Europe/Paris",
        "Europe/Berlin",
        "Europe/Moscow",
        "America/New_York",
        "America/Chicago",
        "America/Denver",
        "America/Los_Angeles",
        "America/St_Johns",
        "America/Sao_Paulo",
        "America/Santiago",
        "America/Argentina/Buenos_Aires",
        "America/Phoenix",
        "Asia/Kathmandu",
        "Asia/Kolkata",
        "Asia/Tokyo",
        "Asia/Shanghai",
        "Asia/Tehran",
        "Asia/Jerusalem",
        "Asia/Dubai",
        "Asia/Kabul",
        "Australia/Lord_Howe",
        "Australia/Adelaide",
        "Australia/Sydney",
        "Australia/Perth",
        "Pacific/Chatham",
        "Pacific/Auckland",
        "Pacific/Apia",
        "Pacific/Honolulu",
        "Africa/Casablanca",
        "Africa/Accra",
        "Africa/Cairo",
        "Africa/Johannesburg",
        "Etc/GMT+5",
        "GMT",
    ];
    let mut timestamps = vec![
        i64::MIN,
        i64::MAX,
        -1,
        0,
        1,
        -2147483648,
        2147483647,
        2147483648,
        -62198755200,
        -62167219200,
        253402300800,
        67768036191676800,
    ];
    for year in [
        -100, 1, 1800, 1900, 1999, 2000, 2004, 2015, 2020, 2024, 2026, 2100, 2400,
    ] {
        for (month, day) in [(1, 1), (1, 4), (2, 28), (3, 1), (6, 30), (12, 28), (12, 31)] {
            timestamps.push(
                Civil {
                    year,
                    month,
                    day,
                    hour: 23,
                    minute: 59,
                    second: 59,
                }
                .timestamp() as i64,
            );
        }
    }
    for boundary in [
        1585443600, 1603587600, 1710054000, 1730613600, 1728142200, 1712415600, 1325239200,
    ] {
        for delta in [-3601, -1, 0, 1, 1800, 3601] {
            timestamps.push(boundary + delta);
        }
    }
    for zone in zones {
        let mut matrix = Matrix::new();
        matrix.case("date_default_timezone_set", vec![Value::from(zone)]);
        for &timestamp in &timestamps {
            for letter in b"dDjlSNwzWoFmMntLyYXxaABgGhHisuvIeOPpTZcrU?\\" {
                let args = vec![Value::from(vec![*letter]), Value::Int(timestamp)];
                matrix.case("date", args.clone());
                matrix.case("gmdate", args.clone());
                matrix.case("idate", args);
            }
            matrix.case("getdate", vec![Value::Int(timestamp)]);
            for associative in [false, true] {
                matrix.case(
                    "localtime",
                    vec![Value::Int(timestamp), Value::Bool(associative)],
                );
            }
            matrix.case(
                "date",
                vec![
                    Value::from(b"\\Y Y\0m\xff\\".to_vec()),
                    Value::Int(timestamp),
                ],
            );
        }
        matrix.finish(&format!("date-formatting-{}", zone.replace('/', "-")));
    }
}

#[test]
fn validation_normalization_and_hostile_arguments() {
    let mut matrix = Matrix::new();
    for year in [
        i64::MIN,
        -400,
        -1,
        0,
        1,
        4,
        69,
        70,
        99,
        100,
        1900,
        2000,
        2024,
        2100,
        32767,
        32768,
        i64::MAX,
    ] {
        for month in [-1, 0, 1, 2, 4, 12, 13, i64::MAX] {
            for day in [-1, 0, 1, 28, 29, 30, 31, 32, i64::MAX] {
                matrix.case(
                    "checkdate",
                    vec![Value::Int(month), Value::Int(day), Value::Int(year)],
                );
                matrix.case(
                    "gmmktime",
                    vec![
                        Value::Int(0),
                        Value::Int(0),
                        Value::Int(0),
                        Value::Int(month),
                        Value::Int(day),
                        Value::Int(year),
                    ],
                );
            }
        }
    }
    for value in [
        Value::Null,
        Value::Bool(false),
        Value::Bool(true),
        Value::Int(123),
        Value::Float(1.5),
        Value::Float(f64::NAN),
        Value::Float(f64::INFINITY),
        Value::from(""),
        Value::from("not a date"),
        Value::from(vec![0xff]),
        Value::from(phera_extension_api::PhpArray::new()),
    ] {
        for function in ["date", "gmdate", "idate"] {
            matrix.case(function, vec![value.clone(), Value::Int(0)]);
            // Null uses the real clock. A literal format still verifies its
            // nullable signature without depending on two processes' wall clocks.
            let format = if value.is_null() { "?" } else { "c" };
            matrix.case(function, vec![Value::from(format), value.clone()]);
            matrix.case(function, vec![]);
            matrix.case(
                function,
                vec![Value::from("c"), Value::Int(0), Value::Int(1)],
            );
        }
        matrix.case(
            "checkdate",
            vec![value.clone(), Value::Int(1), Value::Int(2024)],
        );
        matrix.case("date_default_timezone_set", vec![value]);
    }
    matrix.finish("date-validation");
}
