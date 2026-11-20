//! Calendar fields are local civil values with PHP's zero-based month numbering.

use icu_calendar::{AnyCalendar, AnyCalendarKind, Date, cal::Iso, types::DateFields};
use phera_ext_date::calendar::{Civil, days, leap, month_days};
use phera_extension_api::{Value, args};

use super::State;
use crate::timezone;

fn convert(state: &State, civil: Civil) -> Option<Date<AnyCalendar>> {
    let mut kind = state.kind;
    if kind == AnyCalendarKind::Gregorian && state.instant < state.gregorian_change {
        kind = AnyCalendarKind::Julian;
    }
    let calendar = AnyCalendar::new(kind);
    let date = Date::try_new_iso(
        i32::try_from(civil.year).ok()?,
        civil.month as u8,
        civil.day as u8,
    )
    .ok()?;
    Some(date.to_calendar(calendar))
}

pub fn read(state: &State, field: i64) -> Option<i64> {
    let instant = state.as_datetime();
    let civil = instant.format().civil;
    let tz = || timezone::offsets(&state.zone, instant.timestamp);
    let converted = || convert(state, civil);
    let value = match field {
        0 => i64::from(civil.year > 0),
        1 => i64::from(converted()?.year().era_year_or_related_iso()),
        2 => i64::from(converted()?.month().ordinal) - 1,
        3 => civil.iso_week().1,
        4 => (civil.day - 1).div_euclid(7) + 1,
        5 => i64::from(converted()?.day_of_month().0),
        6 => i64::from(converted()?.day_of_year().0),
        7 => civil.weekday() + 1,
        8 => (civil.day - 1).div_euclid(7) + 1,
        9 => civil.hour / 12,
        10 => civil.hour % 12,
        11 => civil.hour,
        12 => civil.minute,
        13 => civil.second,
        14 => instant.micros / 1000,
        15 => tz().0,
        16 => tz().1,
        17 => civil.iso_week().0,
        18 => (civil.weekday() + 1 - state.first).rem_euclid(7) + 1,
        19 => i64::from(converted()?.year().extended_year()),
        20 => i64::try_from(days(civil.year, civil.month, civil.day)).ok()? + 2_440_588,
        21 => ((civil.hour * 60 + civil.minute) * 60 + civil.second) * 1000 + instant.micros / 1000,
        22 => i64::from(converted()?.month().to_input().is_leap()),
        _ => return None,
    };
    Some(value)
}

fn civil(state: &State) -> Civil {
    let raw = state.as_datetime().format().civil;
    convert(state, raw).map_or(raw, |date| Civil {
        year: i64::from(date.year().extended_year()),
        month: i64::from(date.month().ordinal),
        day: i64::from(date.day_of_month().0),
        ..raw
    })
}

pub fn components(values: &[Value], current: Civil, preserve_clock: bool) -> Civil {
    Civil {
        year: args::int(values, 0).unwrap_or(current.year),
        month: args::int(values, 1).map_or(current.month, |month| month.saturating_add(1)),
        day: args::int(values, 2).unwrap_or(1),
        hour: args::int(values, 3).unwrap_or(if preserve_clock { current.hour } else { 0 }),
        minute: args::int(values, 4).unwrap_or(if preserve_clock { current.minute } else { 0 }),
        second: args::int(values, 5).unwrap_or(if preserve_clock { current.second } else { 0 }),
    }
}

pub fn set_civil(state: &mut State, civil: Civil, milliseconds: i64) -> bool {
    let valid = (1..=12).contains(&civil.month)
        && (1..=month_days(civil.year, civil.month)).contains(&civil.day)
        && (0..24).contains(&civil.hour)
        && (0..60).contains(&civil.minute)
        && (0..60).contains(&civil.second)
        && (0..1000).contains(&milliseconds);
    if !state.lenient && state.date_type() == 1 && !valid {
        return false;
    }
    let mut kind = state.kind;
    if kind == AnyCalendarKind::Gregorian
        && (civil.timestamp() as f64) * 1000.0 < state.gregorian_change
    {
        kind = AnyCalendarKind::Julian;
    }
    let civil = if matches!(kind, AnyCalendarKind::Gregorian | AnyCalendarKind::Iso) {
        let seconds = civil
            .timestamp()
            .clamp(i128::from(i64::MIN), i128::from(i64::MAX));
        Civil::from_timestamp(seconds as i64)
    } else {
        let calendar = AnyCalendar::new(kind);
        let mut fields = DateFields::default();
        fields.extended_year = i32::try_from(civil.year).ok();
        fields.ordinal_month = u8::try_from(civil.month).ok();
        fields.day = u8::try_from(civil.day).ok();
        let Ok(date) = Date::try_from_fields(fields, Default::default(), calendar) else {
            return false;
        };
        let date = date.to_calendar(Iso);
        Civil {
            year: i64::from(date.year().extended_year()),
            month: i64::from(date.month().ordinal),
            day: i64::from(date.day_of_month().0),
            ..civil
        }
    };
    state.instant = state.zone.resolve(civil, None) as f64 * 1000.0 + milliseconds as f64;
    true
}

pub fn set(state: &mut State, values: &[Value]) -> bool {
    if args::int(values, 2).is_some() {
        let date = components(values, civil(state), args::int(values, 3).is_none());
        let done = set_civil(state, date, 0);
        if done {
            state.unset = 0;
        }
        return done;
    }
    let field = args::int(values, 0).unwrap_or(-1);
    let amount = args::int(values, 1).unwrap_or_default();
    let updated = alter(state, field, amount, false);
    if updated && (0..23).contains(&field) {
        state.unset &= !(1 << field);
    }
    updated
}

pub fn date(state: &mut State, values: &[Value], preserve_clock: bool) -> bool {
    let civil = components(values, civil(state), preserve_clock);
    let milliseconds = if preserve_clock {
        read(state, 14).unwrap_or(0)
    } else {
        0
    };
    set_civil(state, civil, milliseconds)
}

pub fn alter(state: &mut State, field: i64, amount: i64, add: bool) -> bool {
    let mut date = civil(state);
    let mut ms = read(state, 14).unwrap_or(0);
    let changed = |field: i64| {
        if add {
            field.saturating_add(amount)
        } else {
            amount
        }
    };
    match field {
        0 if !add => {
            date.year = if amount == 0 {
                1 - date.year
            } else {
                date.year.abs()
            }
        }
        1 | 19 => date.year = changed(date.year),
        2 => {
            date.month = changed(date.month - 1).saturating_add(1);
            if add && state.date_type() == 1 {
                date.year = date.year.saturating_add((date.month - 1).div_euclid(12));
                date.month = (date.month - 1).rem_euclid(12) + 1;
                date.day = date.day.min(month_days(date.year, date.month));
            }
        }
        3 | 4 | 8 if add => date.day = date.day.saturating_add(amount.saturating_mul(7)),
        5 => date.day = changed(date.day),
        6 if !add => {
            date.month = 1;
            date.day = amount;
        }
        6 | 7 if add => date.day = changed(date.day),
        7 if !add => {
            date.day = date
                .day
                .saturating_add(amount.saturating_sub(date.weekday() + 1))
        }
        9 => {
            date.hour = (date.hour % 12).saturating_add(changed(date.hour / 12).saturating_mul(12))
        }
        10 => date.hour = (date.hour - date.hour % 12).saturating_add(changed(date.hour % 12)),
        11 => date.hour = changed(date.hour),
        12 => date.minute = changed(date.minute),
        13 => date.second = changed(date.second),
        14 => ms = changed(ms),
        _ => return false,
    }
    if add && (10..=14).contains(&field) {
        let unit = match field {
            10 | 11 => 3_600_000.0,
            12 => 60_000.0,
            13 => 1000.0,
            _ => 1.0,
        };
        state.instant += amount as f64 * unit;
        return true;
    }
    set_civil(state, date, ms)
}

pub fn limits(state: &State, field: i64, maximal: bool, actual: bool) -> Option<i64> {
    let date = civil(state);
    let (min, max) = match field {
        0 => (0, 1),
        1 => (1, 5_838_270),
        2 => (0, 11),
        3 => (1, 53),
        4 | 8 => (0, 6),
        5 => (
            1,
            if actual {
                month_days(date.year, date.month)
            } else {
                31
            },
        ),
        6 => (1, if actual && !leap(date.year) { 365 } else { 366 }),
        7 | 18 => (1, 7),
        9 | 22 => (0, 1),
        10 => (0, 11),
        11 => (0, 23),
        12 | 13 => (0, 59),
        14 => (0, 999),
        15 => (-57_600_000, 43_200_000),
        16 => (0, 7_200_000),
        17 | 19 => (-5_838_269, 5_838_270),
        20 => (-2_130_706_432, 2_130_706_432),
        21 => (0, 86_399_999),
        _ => return None,
    };
    Some(if maximal { max } else { min })
}
