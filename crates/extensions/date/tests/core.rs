//! Oracle evidence for the calendar and timezone core before exposing formatters.
//! PHP runs with `-n`, and both sides read embedded IANA data, never host zoneinfo.
use phera_ext_date::{Date, calendar::Civil, zone::Zone};
use phera_extension_api::testing::{TestContext, call, oracle};
use phera_extension_api::{Registry, Value};

#[test]
fn request_timezone_and_invalid_identifiers() {
    let mut registry = Registry::new();
    registry.load(&Date);
    let mut context = TestContext::default();
    let mut script = format!("<?php\n{}", oracle::PHP_PREAMBLE);
    let mut expected = Vec::new();
    let mut labels = Vec::new();
    for name in [
        "UTC",
        "utc",
        "Europe/brussels",
        "GMT",
        "EST",
        "CET",
        "Factory",
        "US/Eastern",
        "+02:00",
        "posixrules",
        "",
        "UTC\0x",
        "bad\0name",
    ] {
        for (function, args) in [
            ("date_default_timezone_set", vec![Value::from(name)]),
            ("date_default_timezone_get", vec![]),
        ] {
            context.diagnostics.entries.clear();
            let expression = format!(
                "{function}({})",
                args.iter()
                    .map(oracle::php_literal)
                    .collect::<Vec<_>>()
                    .join(",")
            );
            script.push_str(&format!("run('case',fn()=>{expression});\n"));
            let result = call(&registry, &mut context, function, args);
            expected.push(oracle::encode_case(
                "case",
                &result,
                &[],
                &context.diagnostics,
            ));
            labels.push(expression);
        }
    }
    let php = oracle::run_php("date-request-zone", &script).expect("PHP 8.5 oracle required");
    oracle::compare("request zones", &labels, &php, &expected);
    let mut next_request = TestContext::default();
    assert_eq!(
        call(
            &registry,
            &mut next_request,
            "date_default_timezone_get",
            vec![]
        )
        .unwrap(),
        Value::from("UTC")
    );
}

#[test]
fn embedded_zone_calendar_matrix() {
    let mut script = String::from("<?php\n");
    let mut labels = Vec::new();
    let mut rust = Vec::new();
    let timestamps = [
        i64::MIN,
        i64::MIN + 86400,
        -9223372000000000000,
        -62198755200,
        -62167219200,
        -2208988800,
        -2147483648,
        -1,
        0,
        1,
        951782400,
        1585443600,
        1603587600,
        1730613600,
        1790000000,
        2147483647,
        2147483648,
        4102444800,
        7258118400,
        253402300800,
        67768036191676800,
        9223372000000000000,
        i64::MAX - 86400,
        i64::MAX,
    ];
    for name in tzdb_data::TZ_NAMES {
        let zone = Zone::named(name.as_bytes()).expect("embedded zone");
        script.push_str(&format!("date_default_timezone_set('{name}');\n"));
        let mut samples = timestamps.to_vec();
        for year in [
            Civil::from_timestamp(i64::MIN).year,
            Civil::from_timestamp(i64::MAX).year,
        ] {
            for month in 1..=12 {
                let civil = Civil {
                    year,
                    month,
                    day: 1,
                    hour: 12,
                    minute: 0,
                    second: 0,
                };
                if let Ok(timestamp) = i64::try_from(civil.timestamp()) {
                    samples.push(timestamp);
                }
            }
        }
        for transition in zone.rules.transitions() {
            for delta in [-86401, -1, 0, 1, 86401] {
                if let Some(timestamp) = transition.unix_leap_time().checked_add(delta) {
                    samples.push(timestamp);
                }
            }
        }
        for year in [1800, 1900, 1970, 2000, 2024, 2026, 2027, 2038, 2100, 2400] {
            for month in 1..=12 {
                samples.push(
                    Civil {
                        year,
                        month,
                        day: 15,
                        hour: 12,
                        minute: 0,
                        second: 0,
                    }
                    .timestamp() as i64,
                );
            }
        }
        samples.sort_unstable();
        samples.dedup();
        for timestamp in samples {
            let literal = oracle::php_literal(&Value::Int(timestamp));
            script.push_str(&format!(
                "echo date('Y-m-d H:i:s Z I T', {literal}), \"\\n\";\n"
            ));
            let (civil, offset) = zone.local(timestamp);
            let sign = if civil.year < 0 { "-" } else { "" };
            rust.push(format!(
                "{sign}{:04}-{:02}-{:02} {:02}:{:02}:{:02} {} {} {}",
                civil.year.abs(),
                civil.month,
                civil.day,
                civil.hour,
                civil.minute,
                civil.second,
                offset.seconds,
                u8::from(offset.daylight),
                offset.abbreviation
            ));
            labels.push(format!("{name} {timestamp}"));
            assert_eq!(
                Civil::from_timestamp(timestamp).timestamp(),
                i128::from(timestamp)
            );
        }
    }
    script.push_str("fwrite(STDERR, timezone_version_get());\n");
    let php = oracle::run_php("date-calendar-zones", &script).expect("PHP 8.5 oracle required");
    let mismatches: Vec<_> = labels
        .iter()
        .zip(php.iter().zip(&rust))
        .filter(|(_, (php, rust))| php != rust)
        .map(|(label, (php, rust))| format!("{label}\tPHP={php}\tRust={rust}"))
        .collect();
    std::fs::write(
        std::env::temp_dir().join("phera-date-core-mismatches.txt"),
        mismatches.join("\n"),
    )
    .unwrap();
    eprintln!(
        "{} core comparisons; {} mismatches; Rust IANA {} (historical backzone 2026b)",
        labels.len(),
        mismatches.len(),
        jiff_tzdb::VERSION.unwrap_or("unknown")
    );
    oracle::compare("calendar zones", &labels, &php, &rust);
}
