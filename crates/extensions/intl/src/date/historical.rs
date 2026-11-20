//! ICU's Gregorian calendar uses Julian dates before its October 1582 cutover.
//!
//! ```php
//! (new IntlDateFormatter('en', 0, 0, 'UTC', 1, 'yyyy-MM-dd'))->parse('0024-01-01');
//! ```

use icu_calendar::{Date, cal::Julian};
use phera_ext_date::{ZoneKind, calendar::Civil};

const JULIAN_UNTIL: (i64, i64, i64) = (1582, 10, 15);
const UNIX_EPOCH_RATA_DIE: i64 = 719_163;

pub fn resolve(zone: &ZoneKind, civil: Civil) -> i64 {
    if (civil.year, civil.month, civil.day) >= JULIAN_UNTIL {
        return zone.resolve(civil, None);
    }
    let index = civil.month - 1;
    let Some(year) = i32::try_from(civil.year + index.div_euclid(12)).ok() else {
        return zone.resolve(civil, None);
    };
    let Ok(month) = Date::try_new_julian(year, (index.rem_euclid(12) + 1) as u8, 1) else {
        return zone.resolve(civil, None);
    };
    let day = month.to_rata_die().to_i64_date() - UNIX_EPOCH_RATA_DIE + civil.day - 1;
    let Some(seconds) = day.checked_mul(86_400) else {
        return zone.resolve(civil, None);
    };
    let gregorian_date = Civil::from_timestamp(seconds);
    zone.resolve(
        Civil {
            year: gregorian_date.year,
            month: gregorian_date.month,
            day: gregorian_date.day,
            ..civil
        },
        None,
    )
}

/// The local civil fields, with the actual weekday and the calendar's day-of-year.
pub fn fields(civil: Civil) -> (Civil, i64, i64) {
    let weekday = civil.weekday();
    if (civil.year, civil.month, civil.day) < JULIAN_UNTIL
        && let Ok(year) = i32::try_from(civil.year)
        && let Ok(date) = Date::try_new_gregorian(year, civil.month as u8, civil.day as u8)
    {
        let date = date.to_calendar(Julian);
        let civil = Civil {
            year: i64::from(date.year().extended_year()),
            month: i64::from(date.month().ordinal),
            day: i64::from(date.day_of_month().0),
            ..civil
        };
        return (civil, weekday, i64::from(date.day_of_year().0));
    }
    (civil, weekday, civil.yearday() + 1)
}
