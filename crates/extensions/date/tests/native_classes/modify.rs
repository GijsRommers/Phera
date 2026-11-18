//! Relative strings use a fixed base instead of the wall clock.

use phera_extension_api::{Value, testing::oracle::php_literal};

use super::support::Matrix;

const CLASS: &str = "DateTimeImmutable";

#[test]
fn relative_modify_matches_php() {
    let mut modifiers: Vec<Value> = [
        "next monday",
        "previous thursday",
        "first day of next month",
        "last day of previous month",
        "first monday of next month",
        "last friday of this month",
        "next week",
        "this week sunday",
        "last week monday",
        "yesterday",
        "tomorrow",
        "midnight",
        "noon",
        "today",
        "now",
        "next weekday",
        "2 weekdays ago",
        "2024-02-30",
        "12:34:56.987654",
        "24:00",
        "2024-W01-4",
        "2024-060",
        "@-0.000001",
        "@1704067200.999999",
        "2024-10-27 02:30 Europe/Brussels",
        "January 31",
        "February 29 2024",
        "tomorrow midnight",
        "",
        " ",
        "not a date",
        "  2024-01-02 gibberish",
        "12:00 13:00",
        "2020-01-01 01:00 UTC GMT Z",
    ]
    .into_iter()
    .map(Value::from)
    .collect();
    for unit in [
        "years",
        "months",
        "fortnights",
        "weeks",
        "days",
        "hours",
        "minutes",
        "seconds",
        "weekdays",
        "mondays",
        "tuesdays",
        "wednesdays",
        "thursdays",
        "fridays",
        "saturdays",
        "sundays",
    ] {
        for amount in [-31, -12, -7, -2, -1, 0, 1, 2, 5, 7, 12, 31, 48] {
            modifiers.push(format!("{amount:+} {unit}").into());
            modifiers.push(format!("{amount:+} {unit} ago").into());
            modifiers.push(format!("{amount:+} {unit} +5 hours").into());
        }
        for qualifier in [
            "next", "previous", "last", "this", "first", "second", "third",
        ] {
            modifiers.push(format!("{qualifier} {unit}").into());
        }
    }

    for zone in [
        "UTC",
        "Europe/Brussels",
        "America/New_York",
        "America/St_Johns",
        "Europe/Dublin",
        "Australia/Lord_Howe",
        "Australia/Adelaide",
        "Pacific/Chatham",
        "Asia/Kathmandu",
        "Pacific/Apia",
        "Africa/Casablanca",
        "+05:45",
        "-02:30",
        "EDT",
        "NPT",
        "Z",
    ] {
        let mut matrix = Matrix::new();
        let timezone = matrix
            .native
            .new_object("DateTimeZone", vec![zone.into()])
            .unwrap();
        for base in [
            "2024-01-31 23:50:23.123456",
            "2024-02-29 02:30:00.999999",
            "2024-03-30 02:30:00.000001",
            "2024-03-31 03:30:00.111111",
            "2024-04-06 01:45:00.222222",
            "2024-10-26 02:30:00.333333",
            "2024-10-28 02:30:00.444444",
            "2024-11-02 01:30:00.555555",
            "2024-11-04 01:30:00.666666",
            "2024-12-31 23:55:00.777777",
            "1899-09-29 12:00:00.754321",
            "2011-12-29 08:00:00.251234",
            "2099-10-04 01:45:00.888888",
            "2099-12-31 12:00:00.987654",
            "1900-01-01 00:07:00.000000",
            "2000-03-01 03:00:00.101010",
        ] {
            matrix.setup(&format!(
                "$d = new DateTimeImmutable({base:?}, new DateTimeZone({zone:?}));"
            ));
            let date = matrix
                .native
                .new_object(CLASS, vec![base.into(), timezone.clone()])
                .unwrap();
            for modifier in &modifiers {
                let literal = php_literal(modifier);
                let result = matrix
                    .native
                    .method(&date, "modify", vec![modifier.clone()]);
                let output = matrix.native.date_output(result);
                matrix.case(
                    format!("{base} {zone}: {modifier:?}"),
                    &format!("date_value($d->modify({literal}))"),
                    output,
                );
            }
        }
        matrix.finish(&format!(
            "date-modify-{}",
            zone.replace(['/', '+', ':'], "_")
        ));
    }
}
