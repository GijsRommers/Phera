//! PHP normalizes overflowing components: `gmmktime(0, 0, 0, 13, 1, 2024)` is 2025.
//! Explicit wrapping also reproduces timelib's signed arithmetic at integer extremes.

use crate::calendar::{Civil, month_days};

pub(crate) fn civil(mut date: Civil) -> Civil {
    carry(&mut date.second, &mut date.minute, 60);
    carry(&mut date.minute, &mut date.hour, 60);
    carry(&mut date.hour, &mut date.day, 24);
    normalize_month(&mut date);
    if date.day >= 146097 || date.day <= -146097 {
        date.year = date.year.wrapping_add(400 * (date.day / 146097));
        date.day %= 146097;
    }
    while date.day <= 0 {
        date.month -= 1;
        normalize_month(&mut date);
        date.day += month_days(date.year, date.month);
    }
    while date.day > month_days(date.year, date.month) {
        date.day -= month_days(date.year, date.month);
        date.month += 1;
        normalize_month(&mut date);
    }
    date
}

pub fn timestamp(date: Civil) -> i64 {
    let date = civil(date);
    let year = date.year.wrapping_sub(i64::from(date.month <= 2));
    let era = if year >= 0 {
        year
    } else {
        year.wrapping_sub(399)
    } / 400;
    let year_of_era = year.wrapping_sub(era.wrapping_mul(400));
    let march_month = date.month + if date.month > 2 { -3 } else { 9 };
    let day_of_year = (153 * march_month + 2) / 5 + date.day - 1;
    let day_of_era = year_of_era
        .wrapping_mul(365)
        .wrapping_add(year_of_era / 4)
        .wrapping_sub(year_of_era / 100)
        .wrapping_add(day_of_year);
    let days = era
        .wrapping_mul(146097)
        .wrapping_add(day_of_era)
        .wrapping_sub(719468);
    days.wrapping_mul(86400)
        .wrapping_add(date.hour * 3600 + date.minute * 60 + date.second)
}

fn carry(value: &mut i64, larger: &mut i64, base: i64) {
    *larger = larger.wrapping_add(value.div_euclid(base));
    *value = value.rem_euclid(base);
}

fn normalize_month(date: &mut Civil) {
    let month = i128::from(date.month) - 1;
    date.year = date.year.wrapping_add(month.div_euclid(12) as i64);
    date.month = (month.rem_euclid(12) + 1) as i64;
}
