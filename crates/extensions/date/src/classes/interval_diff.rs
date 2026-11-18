//! `$start->diff($end)` returns calendar fields and a separate total of complete days.

use super::{instant::Instant, transition_list, zone_kind::ZoneKind};
use crate::calendar::{Civil, days, month_days};

pub struct Difference {
    pub fields: [i64; 7],
    pub days: i64,
    pub invert: i64,
}

pub fn difference<'a>(
    mut first: &'a Instant,
    mut second: &'a Instant,
    absolute: bool,
) -> Difference {
    let identifier = match (&first.zone, &second.zone) {
        (ZoneKind::Identifier(first), ZoneKind::Identifier(second))
            if first.name == second.name =>
        {
            Some(first)
        }
        _ => None,
    };
    let mut from = first.format();
    let mut to = second.format();
    let reversed = if identifier.is_some() {
        let key = |date: &crate::format::DateFormat| {
            let Civil {
                year,
                month,
                day,
                hour,
                minute,
                second,
            } = date.civil;
            [
                year,
                month,
                day,
                hour,
                minute,
                second,
                i64::from(date.microsecond),
            ]
        };
        key(&from) > key(&to)
    } else {
        (first.timestamp, first.micros) > (second.timestamp, second.micros)
    };
    if reversed {
        std::mem::swap(&mut first, &mut second);
        std::mem::swap(&mut from, &mut to);
    }
    let a = from.civil;
    let b = to.civil;
    let mut result = Difference {
        fields: [
            b.year - a.year,
            b.month - a.month,
            b.day - a.day,
            b.hour - a.hour,
            b.minute - a.minute,
            b.second - a.second,
            second.micros - first.micros,
        ],
        days: total_days(first, second, a, b, identifier.is_some()),
        invert: i64::from(reversed),
    };
    let offset_change = i64::from(to.offset.seconds) - i64::from(from.offset.seconds);
    if identifier.is_some() {
        if second.timestamp < first.timestamp {
            let flipped = (result.fields[4] * 60 + result.fields[5] - offset_change).abs();
            result.fields[3] = flipped / 3600;
            result.fields[4] = flipped % 3600 / 60;
            result.fields[5] = flipped % 60;
            result.invert = 1 - result.invert;
        }
    } else {
        result.fields[5] -= offset_change;
    }
    normalize(&mut result, a, b);
    if let Some(zone) = identifier {
        let elapsed = second.timestamp.wrapping_sub(first.timestamp);
        let local = elapsed.wrapping_add(offset_change);
        let daylight = (from.offset.daylight, to.offset.daylight);
        let transition = transition_list::latest(zone, second.timestamp);
        let first_plus_day = first.timestamp.wrapping_add(86_400);
        if (daylight == (true, false) && local < 86_400)
            || (daylight == (false, true)
                && !(first_plus_day > transition
                    && first_plus_day <= transition.wrapping_add(offset_change))
                && second.timestamp >= transition
                && local % 86_400 > second.timestamp.wrapping_sub(transition))
        {
            result.fields[3] -= offset_change / 3600;
            result.fields[4] -= offset_change % 3600 / 60;
        } else if from.offset.daylight == to.offset.daylight && elapsed >= 86_400 {
            let probe = second.timestamp.wrapping_sub(i64::from(to.offset.seconds));
            let transition = transition_list::latest(zone, probe);
            let correction = i64::from(from.offset.seconds) - i64::from(zone.offset(probe).seconds);
            if second.timestamp >= transition.wrapping_sub(correction)
                && second.timestamp < transition
            {
                result.fields[2] -= 1;
                result.fields[3] = 24;
            }
        }
    }
    if absolute {
        result.invert = 0;
    }
    result
}

fn total_days(first: &Instant, second: &Instant, a: Civil, b: Civil, same_id: bool) -> i64 {
    let same_fixed = first.zone.kind() == second.zone.kind()
        && first.zone.kind() != 3
        && first.zone.offset(0).seconds == second.zone.offset(0).seconds;
    if same_id || same_fixed {
        let mut total = (days(a.year, a.month, a.day) - days(b.year, b.month, b.day)).abs() as i32;
        let from = (a.hour, a.minute, a.second, first.micros);
        let to = (b.hour, b.minute, b.second, second.micros);
        let forward = (first.timestamp, first.micros) < (second.timestamp, second.micros);
        if total > 0 && ((forward && to < from) || (!forward && from < to)) {
            total -= 1;
        }
        i64::from(total)
    } else {
        let elapsed = first.timestamp.wrapping_sub(second.timestamp);
        let total = (elapsed as f64 / 86_400.0).abs().trunc();
        if total > f64::from(i32::MAX) {
            i64::from(if cfg!(target_arch = "aarch64") {
                i32::MAX
            } else {
                i32::MIN
            })
        } else {
            total as i64
        }
    }
}

fn normalize(result: &mut Difference, start: Civil, end: Civil) {
    let fields = &mut result.fields;
    for (lower, base) in [(6, 1_000_000), (5, 60), (4, 60), (3, 24), (1, 12)] {
        fields[lower - 1] += fields[lower].div_euclid(base);
        fields[lower] = fields[lower].rem_euclid(base);
    }
    let mut anchor = if result.invert == 0 { end } else { start };
    // Positive and inverted differences borrow days from opposite month boundaries.
    while fields[2] < 0 {
        if result.invert == 0 {
            step_month(&mut anchor, -1);
        }
        fields[2] += month_days(anchor.year, anchor.month);
        fields[1] -= 1;
        if result.invert != 0 {
            step_month(&mut anchor, 1);
        }
    }
    fields[0] += fields[1].div_euclid(12);
    fields[1] = fields[1].rem_euclid(12);
}

fn step_month(date: &mut Civil, step: i64) {
    let month = date.month - 1 + step;
    date.year += month.div_euclid(12);
    date.month = month.rem_euclid(12) + 1;
}
