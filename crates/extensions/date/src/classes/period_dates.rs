//! `foreach ($period as $date)` advances relative fields using the start's timezone.
//! This cursor is compiled into the native tests until the VM accepts native iterators.

use super::{instant::Instant, interval_arithmetic::apply_relative, period::Period};

fn dates(period: &Period) -> impl Iterator<Item = Instant> + '_ {
    let mut index = 0;
    let mut current = period.start.clone();
    if !period.include_start
        && let Some(current) = &mut current
    {
        apply_relative(current, period.interval.relative.clone());
    }
    std::iter::from_fn(move || {
        let date = current.as_mut()?;
        let include = if let Some(end) = &period.end {
            let time = (date.timestamp, date.micros);
            let boundary = (end.timestamp, end.micros);
            time < boundary || (period.include_end && time == boundary)
        } else {
            index < period.recurrences
        };
        if include {
            let next = date.clone();
            apply_relative(date, period.interval.relative.clone());
            index += 1;
            Some(next)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use crate::classes::{datetime_factory::parse_in_zone, interval};
    use crate::{classes::zone_kind::ZoneKind, zone::Zone};
    use phera_extension_api::{
        Value,
        testing::{TestContext, oracle},
    };

    use super::*;

    const FORMAT: &str = r"Y-m-d\TH:i:s.uP e I";

    #[test]
    fn date_lists_match_php() {
        let mut script = "<?php\n".to_owned();
        let mut actual = Vec::new();
        let mut labels = Vec::new();
        let mut context = TestContext::default();
        for name in [
            "UTC",
            "Europe/Brussels",
            "America/New_York",
            "Australia/Lord_Howe",
            "Pacific/Chatham",
            "Asia/Kathmandu",
            "Australia/Adelaide",
            "Pacific/Apia",
        ] {
            let zone = ZoneKind::Identifier(Zone::named(name.as_bytes()).expect("test zone"));
            for start in [
                "2024-01-31 02:15:20.123456",
                "2024-03-28 02:30:00.000001",
                "2024-10-24 02:15:00.000001",
                "2024-04-06 01:45:00.999999",
                "2024-09-26 02:45:00.999999",
                "2011-12-28 00:00:00.123456",
                "1969-12-30 23:59:59.123456",
                "2099-12-31 07:00:00.123456",
            ] {
                let date =
                    parse_in_zone(&mut context, start.as_bytes(), Ok(Some(zone.clone())), None)
                        .expect("context")
                        .expect("start");
                let end = format!("@{}.999999", date.timestamp + 89 * 86_400);
                let end_date = parse_in_zone(&mut context, end.as_bytes(), Ok(None), None)
                    .expect("context")
                    .expect("end");
                for relative in ["1 day", "1 month", "next Monday", "2 weekdays", "37 hours"] {
                    let interval = interval::relative(relative.as_bytes(), true).unwrap();
                    for options in 0..=3 {
                        for end_bound in [false, true] {
                            let (count, end_date) = if end_bound {
                                (0, Some(end_date.clone()))
                            } else {
                                (7, None)
                            };
                            let mut period = Period::new(
                                date.clone(),
                                interval.clone(),
                                end_date,
                                count,
                                "DateTimeImmutable",
                            );
                            period.include_start = options & 1 == 0;
                            period.include_end = options & 2 != 0;
                            period.recurrences +=
                                i64::from(period.include_start) + i64::from(period.include_end);
                            let bound = if end_bound {
                                format!("new DateTimeImmutable({end:?})")
                            } else {
                                count.to_string()
                            };
                            writeln!(
                                script,
                                "\
$p = new DatePeriod(new DateTimeImmutable({start:?}, new DateTimeZone({name:?})), \
DateInterval::createFromDateString({relative:?}), {bound}, {options});
foreach ($p as $d) {{ echo bin2hex($d->format('Y-m-d\\TH:i:s.uP e I')), ','; }}
echo \"\\n\";"
                            )
                            .unwrap();
                            labels.push(format!("{name}; {start}; {relative}; {bound}; {options}"));
                            let mut line = String::new();
                            for date in dates(&period) {
                                let text = Value::from(date.format().render(FORMAT.as_bytes()));
                                let encoded = oracle::encode(&text);
                                line.push_str(encoded.strip_prefix("S:").expect("encoded string"));
                                line.push(',');
                            }
                            actual.push(line);
                        }
                    }
                }
            }
        }
        let expected = oracle::run_php("date-period-date-lists", &script).expect("PHP oracle");
        oracle::compare("date period date lists", &labels, &expected, &actual);
    }
}
