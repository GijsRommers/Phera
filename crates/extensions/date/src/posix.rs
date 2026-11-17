//! Extend embedded POSIX rules to PHP's 64-bit years: `date('I', PHP_INT_MAX)`
//! also reproduces timelib's wrapped next-year transition search.
use crate::calendar::{Civil, days, leap, month_days};
use tz::LocalTimeType;
use tz::timezone::{RuleDay, TransitionRule};

pub fn offset(rule: &TransitionRule, timestamp: i64) -> Option<&LocalTimeType> {
    let TransitionRule::Alternate(rule) = rule else {
        let TransitionRule::Fixed(kind) = rule else {
            return None;
        };
        return Some(kind);
    };
    let year = Civil::from_timestamp(timestamp).year;
    let mut transitions = Vec::with_capacity(6);
    for year in [year - 1, year, year + 1] {
        let start = transition(
            year,
            rule.dst_start(),
            rule.dst_start_time(),
            rule.std().ut_offset(),
        );
        let end = transition(
            year,
            rule.dst_end(),
            rule.dst_end_time(),
            rule.dst().ut_offset(),
        );
        if start < end {
            transitions.extend([(start, rule.dst()), (end, rule.std())]);
        } else {
            transitions.extend([(end, rule.std()), (start, rule.dst())]);
        }
    }
    for index in 1..transitions.len() {
        if timestamp < transitions[index].0 {
            return Some(transitions[index - 1].1);
        }
    }
    None
}

fn transition(year: i64, day: &RuleDay, clock: i32, offset: i32) -> i64 {
    let day_number = match day {
        RuleDay::Julian1WithoutLeap(day) => {
            days(year, 1, 1) + i128::from(day.get()) - 1 + i128::from(day.get() >= 60 && leap(year))
        }
        RuleDay::Julian0WithLeap(day) => days(year, 1, 1) + i128::from(day.get()),
        RuleDay::MonthWeekDay(rule) => {
            let first = Civil {
                year,
                month: i64::from(rule.month()),
                day: 1,
                hour: 0,
                minute: 0,
                second: 0,
            };
            // timelib's POSIX parser narrows only this Zeller calculation
            // to a C int, even while the calendar retains its full-width year.
            let short_year = (year - i64::from(rule.month() <= 2)) as i32;
            let century = short_year / 100;
            let within_century = short_year % 100;
            let march_month = (i32::from(rule.month()) + 9) % 12 + 1;
            let weekday = ((26 * march_month - 2) / 10
                + 1
                + within_century
                + within_century / 4
                + century / 4
                - 2 * century)
                .rem_euclid(7);
            let mut day = 1
                + (i64::from(rule.week_day()) - i64::from(weekday)).rem_euclid(7)
                + (i64::from(rule.week()) - 1) * 7;
            if day > month_days(year, first.month) {
                day -= 7;
            }
            days(year, first.month, day)
        }
    };
    (day_number * 86400 + i128::from(clock) - i128::from(offset)) as i64
}
