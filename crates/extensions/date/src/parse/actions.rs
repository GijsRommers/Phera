//! Match application follows timelib's order: `today 12:00` can set the clock again.
use super::state::State;
use super::token::{Action, Keyword, Special, Unit, WeekdayBehavior};
use crate::parse_format::result::ParsedZone;

pub fn apply(state: &mut State, action: Action, input: &[u8], position: usize) {
    match action {
        Action::Keyword(keyword) => keyword_apply(state, keyword),
        Action::Date(date) => {
            state.date(date, position);
        }
        Action::Time(time) => {
            state.time(time, position);
        }
        Action::Compound(date, time, time_first) => {
            let first = if time_first {
                state.time(
                    super::token::TimeToken {
                        fields: [0; 4],
                        zone: None,
                    },
                    position,
                )
            } else {
                state.date(date, position)
            };
            if !first {
                return;
            }
            let success = if time_first {
                state.date(date, position)
            } else {
                state.time(time, position)
            };
            if success {
                if time_first {
                    let [hour, minute, second, _] = time.fields;
                    state.parsed.fields[3..].copy_from_slice(&[hour, minute, second].map(Some));
                    state.parsed.microsecond = Some(time.fields[3]);
                }
                if let Some(zone) = time.zone {
                    state.zone(&input[zone..], position, false);
                }
            }
        }
        Action::Year(year) => state.parsed.fields[0] = Some(year),
        Action::Gnu(hour, minute, year) => {
            match state.have_time {
                0 => {
                    state.parsed.fields[3..].copy_from_slice(&[Some(hour), Some(minute), Some(0)]);
                }
                1 => state.parsed.fields[0] = Some(year),
                _ => {
                    state.error(position, "Double time specification");
                    return;
                }
            }
            state.have_time += 1;
        }
        Action::Zone => state.zone(input, position, true),
        Action::Epoch => epoch(state, input, position),
        Action::Week(year, week, weekday) => {
            if !state.date(
                super::token::DateToken::new(Some(year), 1, Some(1)),
                position,
            ) {
                return;
            }
            state.relative = true;
            let jan4 = (crate::calendar::days(year, 1, 4) + 4).rem_euclid(7);
            state.delta[2] = (3 - (jan4 + 6) % 7) as i64 + (week - 1) * 7 + weekday - 1;
        }
        Action::BareWeekday(day) => {
            state.relative = true;
            state.weekday = Some(day);
            state.midnight();
            if state.behavior != WeekdayBehavior::ThisWeek {
                state.behavior = WeekdayBehavior::OnOrAfter;
            }
        }
        Action::Clock(hour, minute) => {
            state.midnight();
            state.have_time = 1;
            state.parsed.fields[3] = Some(hour);
            state.parsed.fields[4] = Some(minute);
        }
        Action::Relative {
            amount,
            unit,
            behavior,
            reset,
            week,
            of,
        } => {
            state.relative = true;
            if of {
                state.special = if amount > 0 {
                    Special::FirstWeekday
                } else {
                    Special::LastWeekday
                };
            }
            match unit {
                Unit::Field(field, multiplier) => {
                    let amount = amount.wrapping_mul(multiplier);
                    let (value, overflow) = state.delta[field].overflowing_add(amount);
                    state.delta[field] = value;
                    if overflow {
                        state.error(position, "Number out of range");
                    }
                }
                Unit::Weekday(day) => {
                    if reset {
                        state.midnight();
                    }
                    state.delta[2] = state.delta[2].wrapping_add(
                        (if amount > 0 { amount - 1 } else { amount }).wrapping_mul(7),
                    );
                    state.weekday = Some(day);
                    state.behavior = if of && amount > 0 {
                        WeekdayBehavior::OnOrAfter
                    } else {
                        behavior
                    };
                }
                Unit::Weekdays => {
                    if reset {
                        state.midnight();
                    }
                    state.special = Special::Weekdays(amount);
                }
            }
            if week {
                state.behavior = WeekdayBehavior::ThisWeek;
                state.weekday.get_or_insert(1);
            }
        }
    }
}

fn keyword_apply(state: &mut State, keyword: Keyword) {
    match keyword {
        Keyword::Now => {}
        Keyword::Midnight => state.midnight(),
        Keyword::Noon => {
            state.midnight();
            state.have_time = 1;
            state.parsed.fields[3] = Some(12);
        }
        Keyword::Yesterday | Keyword::Tomorrow => {
            state.relative = true;
            state.midnight();
            state.delta[2] = if matches!(keyword, Keyword::Yesterday) {
                -1
            } else {
                1
            };
        }
        Keyword::First | Keyword::Last => {
            state.relative = true;
            state.month_edge = Some(matches!(keyword, Keyword::First));
        }
        Keyword::Ago => {
            for value in &mut state.delta[..6] {
                *value = value.wrapping_neg();
            }
            if let Some(day) = &mut state.weekday {
                *day = if *day == 0 { -7 } else { -*day };
            }
            if let Special::Weekdays(count) = &mut state.special {
                *count = count.wrapping_neg();
            }
        }
    }
}

fn epoch(state: &mut State, input: &[u8], position: usize) {
    state.relative = true;
    state.have_date = false;
    state.parsed.fields[..3].fill(Some(0));
    state.midnight();
    if !state.zone_available(position) {
        return;
    }
    let input = input.strip_prefix(b"@").unwrap_or(input);
    let mut cursor = 0;
    let seconds = epoch_number(input, &mut cursor, 24);
    if seconds.is_none() {
        state.error(position, "Number out of range");
    }
    state.delta[5] = state.delta[5].wrapping_add(seconds.unwrap_or(0));
    if input.contains(&b'.') {
        let before = cursor;
        let fraction = epoch_number(input, &mut cursor, 6);
        if fraction.is_none() {
            state.error(position, "Found unexpected data");
        }
        state.delta[6] = (fraction.unwrap_or(0) as f64 * 10f64.powi(7 - (cursor - before) as i32))
            as i64
            * if input.starts_with(b"-") { -1 } else { 1 };
    }
    state.parsed.fields = [Some(1970), Some(1), Some(1), Some(0), Some(0), Some(0)];
    state.parsed.zone = Some(ParsedZone::Offset(0));
}

fn epoch_number(input: &[u8], cursor: &mut usize, limit: usize) -> Option<i64> {
    while input
        .get(*cursor)
        .is_some_and(|b| !b.is_ascii_digit() && !b"+-".contains(b))
    {
        *cursor += 1;
    }
    let negative = input.get(*cursor) == Some(&b'-');
    if input.get(*cursor).is_some_and(|b| b"+-".contains(b)) {
        *cursor += 1;
    }
    let start = *cursor;
    while *cursor - start < limit && input.get(*cursor).is_some_and(u8::is_ascii_digit) {
        *cursor += 1;
    }
    if start == *cursor {
        return None;
    }
    let magnitude = std::str::from_utf8(&input[start..*cursor])
        .ok()?
        .parse::<i128>()
        .ok()?;
    i64::try_from(magnitude * if negative { -1 } else { 1 }).ok()
}
