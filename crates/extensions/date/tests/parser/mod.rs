//! Deterministic grammar and hostile-input corpus, shared by parser comparison tests.
pub fn examples() -> Vec<String> {
    [
        "",
        " ",
        "now",
        "today",
        "midnight",
        "noon",
        "tomorrow",
        "yesterday",
        "Monday",
        "Sun",
        "next monday",
        "last friday",
        "this sunday",
        "previous wed",
        "2 weekdays",
        "-5 weekdays",
        "first monday of February 2024",
        "last sunday of next month",
        "saturday this week",
        "monday next week",
        "NEXT WEEK",
        "next week",
        "last week",
        "this week",
        "first day of next month",
        "last day of February 2024",
        "2023-01-31 +1 month",
        "2024-01-31 +1 month",
        "back of 7",
        "front of 5 pm",
        "+1 week 2 days",
        "-3 months",
        "2 years ago",
        "+1 microsecond",
        "2000-01-01 12:00 +1 month -2 days",
        "@0",
        "@-1.5",
        "@123.1234567",
        "@1.",
        "2026W101",
        "2026-W10-1",
        "2026-W10",
        "2008.366",
        "2008-366",
        "2008366",
        "2000-01-01",
        "2000-1-1",
        "01/02/03",
        "12/31/2000",
        "1/2",
        "12.11.2000",
        "31-12-2000",
        "31 Dec 2000",
        "December 31, 2000",
        "31st December 2000",
        "Dec 31",
        "December 2000",
        "2000 Dec",
        "Dec-31-2000",
        "2000-Dec-31",
        "31 XII 2000",
        "2000",
        "1200",
        "2400",
        "2500",
        "2026-03",
        "+12000-01-01",
        "-0001-01-01",
        "20000229",
        "20000229T12:34:56",
        "20000229t123456",
        "2000:02:29 12:34:56",
        "2000-02-29T12:34:56.789+02:00",
        "2000-2-29T1:2:3",
        "10/Oct/2000:13:55:36 -0700",
        "Tue, 29 Feb 2000 12:34:56 +0100",
        "Tuesday, 29-Feb-2000 12:34:56 GMT",
        "Tue Feb 29 12:34:56 UTC 2000",
        "May 2 12:30",
        "May 2 12:30:45 EST",
        "May 2 12:30 pm",
        "4 am",
        "12PM",
        "04:08 am",
        "4:08:39:12313am",
        "4:08:39.12313pm",
        "T23",
        "04:08",
        "19.19",
        "04.08.37",
        "T191919",
        "040837",
        "19:19:19.532453",
        "24:00:00",
        "24:59:60",
        "12:34:56.123456789",
        "12:34:56.999999999",
        "UTC",
        "utc",
        "GMT",
        "CET",
        "CEST",
        "Z",
        "Europe/Brussels",
        "America/New_York",
        "+02:30",
        "GMT-0700",
        "+123",
        "+1234",
        "+12:34:56",
        "-123456",
        "(CEST)",
        "UTC UTC UTC",
        "12:00 13:00",
        "2000-01-01 2001-01-01",
        "12:00 today 13:00",
        "May",
        "foo",
        "foobar",
        "bad date",
        "2000-13-32",
        "25:61:99",
        "99/99/9999",
        " 	2000-02-30\r\n",
        "2000-01-01\r12:00",
        "2000-01-01\n12:00",
        "2000\0-01-01",
        "2000-01-01\u{a0}12:00",
        "2000-01-01\u{202f}12:00",
        "µs",
        "é",
        "☃",
        "9223372036854775807",
        "@9223372036854775807",
        "@-9223372036854775808",
        "@9223372036854775808",
        "@-9223372036854775809",
        "--1 day",
        "+-2 weeks",
        "first week",
        "first weeks",
        "weekdays",
        "ago",
        "Monday ago",
        "next Monday ago",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub fn generated() -> Vec<String> {
    let mut inputs = examples();
    for year in [
        0, 1, 69, 70, 99, 100, 1800, 1900, 2000, 2001, 2004, 2023, 2024, 2026, 2100, 2400, 9999,
    ] {
        for month in 0..=13 {
            for day in [0, 1, 2, 27, 28, 29, 30, 31, 32] {
                for format in 0..4 {
                    inputs.push(match format {
                        0 => format!("{year:04}-{month:02}-{day:02}"),
                        1 => format!("{month}/{day}/{year}"),
                        2 => format!("{day}.{month}.{year:04}"),
                        _ => format!("{year:04}{month:02}{day:02}T23:59:60"),
                    });
                }
            }
        }
    }
    for hour in [0, 1, 2, 11, 12, 13, 23, 24, 25, 99] {
        for minute in [0, 1, 9, 59, 60, 99] {
            for second in [0, 1, 59, 60, 61, 99] {
                for suffix in ["", ".1234567", " pm", ".9+02:30"] {
                    inputs.push(format!("{hour}:{minute:02}:{second:02}{suffix}"));
                }
            }
        }
    }
    for unit in [
        "second",
        "minute",
        "hour",
        "day",
        "week",
        "weeks",
        "fortnight",
        "month",
        "year",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
        "weekdays",
        "ms",
        "µs",
        "msecs",
        "usecs",
        "forthnights",
    ] {
        for count in [
            "-13", "-7", "-1", "0", "+1", "2", "5", "12", "first", "second", "third", "fourth",
            "fifth", "sixth", "seventh", "eighth", "ninth", "tenth", "eleventh", "twelfth", "last",
            "next", "previous", "this",
        ] {
            for prefix in ["", "2024-01-31 12:34:56 "] {
                inputs.push(format!("{prefix}{count} {unit}"));
                inputs.push(format!("{prefix}{count} {unit} ago"));
            }
        }
    }
    for zone in [
        "UTC",
        "GMT",
        "utc",
        "Z",
        "EST",
        "EDT",
        "CET",
        "CEST",
        "IST",
        "Asia/Kolkata",
        "Europe/Brussels",
        "America/New_York",
        "Australia/Lord_Howe",
        "Pacific/Apia",
        "+0",
        "-1",
        "+123",
        "+1234",
        "+12:34:56",
        "+24:59",
        "+25:00",
        "Foo/Bar",
    ] {
        for prefix in ["", "2000-02-29 ", "2000-02-29T12:34:56.123456", "12:00 "] {
            for suffix in ["", " UTC", " +1 day"] {
                inputs.push(format!("{prefix}{zone}{suffix}"));
            }
        }
    }
    for separator in [
        " ", "\t", "\n", "\r", "\u{a0}", "\u{202f}", ",", ".", ";", "/", "_", "\0",
    ] {
        for left in ["now", "2000-01-01", "12:00", "UTC", "Monday", "+1 day"] {
            for right in ["now", "2000-01-01", "12:00", "UTC", "Monday", "+1 day"] {
                inputs.push(format!("{left}{separator}{right}"));
            }
        }
    }
    inputs.sort();
    inputs.dedup();
    inputs
}

pub mod adversarial;
