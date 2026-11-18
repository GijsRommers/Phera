//! Include both sides of DST changes, fractional offsets, leap days and month ends.

use phera_extension_api::Value;

use super::support::Matrix;

const CLASS: &str = "DateTimeImmutable";
const BASES: &[&str] = &[
    "1899-01-31 02:15:20.999999",
    "1899-02-28 03:15:20.000001",
    "1900-03-01 10:00:00.000001",
    "1916-05-01 02:00:00.012345",
    "1969-12-30 23:59:59.999999",
    "1969-12-31 23:59:59.999999",
    "1970-01-01 00:00:00.000000",
    "1970-01-01 00:00:00.000001",
    "1999-12-31 23:59:59.123456",
    "2000-02-28 03:01:02.234567",
    "2000-02-29 03:01:02.234567",
    "2000-03-01 03:01:02.234567",
    "2011-12-28 11:00:00.000000",
    "2011-12-29 11:00:00.000000",
    "2011-12-31 11:00:00.000000",
    "2023-01-31 09:12:34.567890",
    "2024-01-29 12:01:02.000001",
    "2024-01-30 12:01:02.000002",
    "2024-01-31 12:01:02.000003",
    "2024-02-28 12:01:02.000004",
    "2024-02-29 12:01:02.000005",
    "2024-03-01 12:01:02.000006",
    "2024-03-09 01:30:00.000010",
    "2024-03-10 01:30:00.000010",
    "2024-03-10 02:30:00.000010",
    "2024-03-10 03:00:00.000010",
    "2024-03-10 03:30:00.000010",
    "2024-03-11 02:30:00.001010",
    "2024-03-30 01:30:00.001010",
    "2024-03-30 02:30:00.001010",
    "2024-03-31 01:30:00.001010",
    "2024-03-31 02:30:00.001010",
    "2024-03-31 03:00:00.001010",
    "2024-03-31 03:30:00.001010",
    "2024-04-01 02:30:00.001010",
    "2024-04-06 01:45:00.001010",
    "2024-04-07 01:30:00.001010",
    "2024-04-07 01:45:00.001010",
    "2024-04-07 02:30:00.001010",
    "2024-04-07 02:45:00.001010",
    "2024-04-07 03:45:00.001010",
    "2024-04-08 01:45:00.001010",
    "2024-09-28 02:45:00.001010",
    "2024-09-29 02:45:00.001010",
    "2024-09-29 03:45:00.001010",
    "2024-09-30 02:45:00.001010",
    "2024-10-05 02:15:00.001010",
    "2024-10-06 02:15:00.001010",
    "2024-10-06 02:45:00.001010",
    "2024-10-07 02:15:00.001010",
    "2024-10-26 02:30:00.001010",
    "2024-10-27 02:00:00.001010",
    "2024-10-27 02:30:00.001010",
    "2024-10-27 03:00:00.001010",
    "2024-10-28 02:30:00.001010",
    "2024-11-02 01:30:00.001010",
    "2024-11-03 01:00:00.001010",
    "2024-11-03 01:30:00.001010",
    "2024-11-03 02:00:00.001010",
    "2024-11-04 01:30:00.001010",
    "2024-12-31 23:59:59.999999",
    "2025-01-01 00:00:00.000000",
    "2099-12-31 23:59:59.999999",
    "2100-03-01 00:00:00.000000",
    "2400-02-29 01:23:45.678901",
    "2400-03-01 01:23:45.678901",
    "@1729989000.000010",
    "@1729992600.000000",
    "@1730611800.000010",
    "@1730615400.000000",
    "@-0.000001",
    "@0.000001",
];

const ZONES: &[(&str, &str)] = &[
    ("UTC", "Europe/Brussels"),
    ("Europe/Brussels", "America/New_York"),
    ("America/New_York", "Australia/Lord_Howe"),
    ("Australia/Lord_Howe", "Pacific/Chatham"),
    ("Pacific/Chatham", "Australia/Adelaide"),
    ("Australia/Adelaide", "Asia/Kathmandu"),
    ("Asia/Kathmandu", "Pacific/Apia"),
    ("Pacific/Apia", "Europe/Dublin"),
    ("Europe/Dublin", "America/St_Johns"),
    ("America/St_Johns", "+05:45"),
    ("+05:45", "EDT"),
    ("EDT", "UTC"),
];

#[test]
fn dst_arithmetic_matches_php() {
    for &(source, target) in ZONES {
        let mut matrix = Matrix::new();
        let first = dates(&mut matrix, source, "a");
        let cross = dates(&mut matrix, target, "b");
        let mut spans = Vec::new();
        for (index, spec) in [
            "P1Y",
            "P1M",
            "P2M",
            "P1W",
            "P1D",
            "P2D",
            "PT1H",
            "PT30M",
            "PT36H",
            "PT24H",
            "P1DT24H",
            "P1M2DT5H15M37S",
            "PT1S",
            "P0D",
            "1 day",
            "24 hours",
            "5 weekdays",
            "last friday",
            "last day of next month",
            "-3 months",
            "next monday",
        ]
        .into_iter()
        .enumerate()
        {
            let (expression, span) = if spec.starts_with('P') {
                (
                    format!("new DateInterval({spec:?})"),
                    matrix
                        .native
                        .new_object("DateInterval", vec![spec.into()])
                        .unwrap(),
                )
            } else {
                (
                    format!("DateInterval::createFromDateString({spec:?})"),
                    matrix
                        .native
                        .call(
                            "DateInterval",
                            None,
                            "createFromDateString",
                            vec![spec.into()],
                        )
                        .unwrap(),
                )
            };
            matrix.setup(&format!("$i{index} = {expression};"));
            spans.push(span);
        }
        for (index, date) in first.iter().enumerate() {
            for (span, interval) in spans.iter().enumerate() {
                for method in ["add", "sub"] {
                    let result = matrix.native.method(date, method, vec![interval.clone()]);
                    let result = matrix.native.date_output(result);
                    let expression = format!("date_value($a[{index}]->{method}($i{span}))");
                    matrix.case(
                        format!("{source} {} {method} interval #{span}", BASES[index]),
                        &expression,
                        result,
                    );
                }
            }
            for (prefix, targets) in [("a", &first), ("b", &cross)] {
                for (other, target_date) in targets.iter().enumerate() {
                    let absolute = index % 3 == 0;
                    let result = matrix.native.method(
                        date,
                        "diff",
                        vec![target_date.clone(), Value::Bool(absolute)],
                    );
                    let result = matrix.native.interval_output(result);
                    let expression = format!(
                        "$a[{index}]->diff(${prefix}[{other}], {absolute})->format(INTERVAL_FORMAT)"
                    );
                    let other_zone = if prefix == "a" { source } else { target };
                    matrix.case(
                        format!(
                            "{source} {} -> {other_zone} {} ({absolute})",
                            BASES[index], BASES[other]
                        ),
                        &expression,
                        result,
                    );
                }
            }
        }
        matrix.finish(&format!(
            "date-arithmetic-{}",
            source.replace(['/', '+', ':'], "_")
        ));
    }
}

fn dates(matrix: &mut Matrix, zone: &str, prefix: &str) -> Vec<Value> {
    matrix.setup(&format!(
        "$zone = new DateTimeZone({zone:?}); ${prefix} = [];"
    ));
    let timezone = matrix
        .native
        .new_object("DateTimeZone", vec![zone.into()])
        .unwrap();
    let mut dates = Vec::new();
    for &base in BASES {
        let expression = format!("(new DateTimeImmutable({base:?}, $zone))->setTimezone($zone)");
        matrix.setup(&format!("${prefix}[] = {expression};"));
        let date = matrix
            .native
            .new_object(CLASS, vec![base.into(), timezone.clone()])
            .unwrap();
        dates.push(
            matrix
                .native
                .method(&date, "setTimezone", vec![timezone.clone()])
                .unwrap(),
        );
    }
    dates
}
