//! Relative application: `2023-01-31 +1 month` rolls into March, without clamping.
use super::{
    state::State,
    token::{Special, WeekdayBehavior},
};
use crate::{calendar::Civil, parse_format::result::ParsedZone, zone::Zone};

pub fn timestamp(state: &State, base: i64, zone: &Zone) -> Option<i64> {
    let seconds = if zone.name == "UTC" {
        0
    } else {
        zone.offset(base).seconds
    };
    let now = Civil::from_timestamp(base.wrapping_add(i64::from(seconds)));
    let defaults = [
        now.year, now.month, now.day, now.hour, now.minute, now.second,
    ];
    let mut fields = state.parsed.fields;
    if state.have_date && state.have_time == 0 {
        fields[3..].fill(Some(0));
    }
    let mut values = [0; 6];
    for ((out, field), default) in values.iter_mut().zip(fields).zip(defaults) {
        *out = field.unwrap_or(default);
    }
    let mut civil = Civil {
        year: values[0],
        month: values[1],
        day: values[2],
        hour: values[3],
        minute: values[4],
        second: values[5],
    };
    let microsecond = state.parsed.microsecond.unwrap_or(0);
    civil.second = civil.second.wrapping_add(microsecond.div_euclid(1_000_000));
    let mut delta = state.delta;
    match state.special {
        Special::FirstWeekday | Special::LastWeekday => {
            civil.day = 1;
            civil.month = civil
                .month
                .wrapping_add(delta[1])
                .wrapping_add(i64::from(state.special == Special::LastWeekday));
            delta[1] = 0;
        }
        _ => {}
    }
    month_edge(&mut civil, state.month_edge);
    normalize(&mut civil);
    if let Some(weekday) = state.weekday {
        adjust_weekday(&mut civil, weekday, state.behavior, delta[2]);
    }
    normalize(&mut civil);
    if state.relative {
        civil.second = civil.second.wrapping_add(delta[5]).wrapping_add(
            microsecond
                .rem_euclid(1_000_000)
                .wrapping_add(delta[6])
                .div_euclid(1_000_000),
        );
        civil.minute = civil.minute.wrapping_add(delta[4]);
        civil.hour = civil.hour.wrapping_add(delta[3]);
        civil.day = civil.day.wrapping_add(delta[2]);
        civil.month = civil.month.wrapping_add(delta[1]);
        civil.year = civil.year.wrapping_add(delta[0]);
    }
    month_edge(&mut civil, state.month_edge);
    normalize(&mut civil);
    if let Special::Weekdays(count) = state.special {
        weekdays(&mut civil, count);
    }
    normalize(&mut civil);
    Some(match &state.parsed.zone {
        Some(ParsedZone::Offset(seconds)) => {
            crate::normalize::timestamp(civil).wrapping_sub(*seconds)
        }
        Some(ParsedZone::Abbreviation {
            seconds, daylight, ..
        }) => {
            crate::normalize::timestamp(civil).wrapping_sub(*seconds + i64::from(*daylight) * 3600)
        }
        Some(ParsedZone::Identifier { name, .. }) => {
            crate::resolve::local_timestamp(&Zone::named(name.as_bytes())?, civil, Some(true))
        }
        _ if zone.name == "UTC" => crate::normalize::timestamp(civil),
        _ => crate::resolve::local_timestamp(zone, civil, None),
    })
}

fn normalize(civil: &mut Civil) {
    *civil = crate::normalize::civil(*civil);
}
fn month_edge(civil: &mut Civil, edge: Option<bool>) {
    if let Some(first) = edge {
        civil.day = i64::from(first);
        if !first {
            civil.month = civil.month.wrapping_add(1);
        }
    }
}
fn adjust_weekday(civil: &mut Civil, mut weekday: i64, behavior: WeekdayBehavior, days: i64) {
    let current = civil.weekday();
    if behavior == WeekdayBehavior::ThisWeek {
        if current == 0 && weekday != 0 {
            weekday -= 7;
        }
        if weekday == 0 && current != 0 {
            weekday = 7;
        }
        civil.day += weekday - current;
        return;
    }
    let mut difference = weekday - current;
    if (days < 0 && difference < 0)
        || (days >= 0 && difference <= -i64::from(behavior == WeekdayBehavior::OnOrAfter))
    {
        difference += 7;
    }
    civil.day += if weekday >= 0 {
        difference
    } else {
        -(7 - (weekday.abs() - current))
    };
}
fn weekdays(civil: &mut Civil, count: i64) {
    let dow = civil.weekday();
    civil.day = civil.day.wrapping_add((count / 5).wrapping_mul(7));
    let remainder = count % 5;
    if count > 0 {
        if remainder == 0 {
            civil.day -= match dow {
                0 => 2,
                6 => 1,
                _ => 0,
            };
        } else if dow == 6 {
            civil.day += 1;
        } else if dow + remainder > 5 {
            civil.day += 2;
        }
    } else if remainder == 0 {
        civil.day += match dow {
            6 => 2,
            0 => 1,
            _ => 0,
        };
    } else if dow == 0 {
        civil.day -= 1;
    } else if dow + remainder < 1 {
        civil.day -= 2;
    }
    civil.day = civil.day.wrapping_add(remainder);
}
