//! Relative words use timelib's units: `next week` means a Monday-based week.
use super::scan::{Cursor, offer};
use super::token::{Action, Candidate, Keyword, Unit, WeekdayBehavior};

pub fn candidates(input: &[u8], best: &mut Option<Candidate>) {
    if input[0].is_ascii_alphabetic() {
        for (rank, word, kind) in [
            (0, "yesterday", Keyword::Yesterday),
            (1, "now", Keyword::Now),
            (2, "noon", Keyword::Noon),
            (3, "midnight", Keyword::Midnight),
            (3, "today", Keyword::Midnight),
            (4, "tomorrow", Keyword::Tomorrow),
            (7, "first day of", Keyword::First),
            (7, "last day of", Keyword::Last),
            (36, "ago", Keyword::Ago),
        ] {
            let mut c = Cursor::new(input);
            if c.word(word.as_bytes()).is_some() {
                offer(best, rank, c, Action::Keyword(kind));
            }
        }
        for front in [false, true] {
            let mut c = Cursor::new(input);
            let clock = (|| {
                c.word(if front { b"front of " } else { b"back of " })?;
                let hour = c.number(1, 2, 24)? - i64::from(front);
                let mut suffix = c;
                suffix.space();
                let hour = if let Some(hour) = suffix.meridian(hour) {
                    c = suffix;
                    hour
                } else {
                    hour
                };
                Some(Action::Clock(hour, if front { 45 } else { 15 }))
            })();
            if let Some(action) = clock {
                offer(best, 8, c, action);
            }
        }
        let mut c = Cursor::new(input);
        if unit(&mut c, true).is_some() {
            let day = match unit(&mut Cursor::new(input), true) {
                Some(Unit::Weekday(day)) => day,
                _ => 1,
            };
            offer(best, 37, c, Action::BareWeekday(day));
        }
        for (word, amount) in [
            ("first", 1),
            ("next", 1),
            ("second", 2),
            ("third", 3),
            ("fourth", 4),
            ("fifth", 5),
            ("sixth", 6),
            ("seventh", 7),
            ("eight", 8),
            ("eighth", 8),
            ("ninth", 9),
            ("tenth", 10),
            ("eleventh", 11),
            ("twelfth", 12),
            ("last", -1),
            ("previous", -1),
            ("this", 0),
        ] {
            let mut c = Cursor::new(input);
            if c.word(word.as_bytes()).is_none() || c.space().is_none() {
                continue;
            }
            let start = c;
            let Some(unit) = unit(&mut c, false) else {
                continue;
            };
            let behavior = if word == "this" {
                WeekdayBehavior::OnOrAfter
            } else {
                WeekdayBehavior::After
            };
            let week = input[start.pos..c.pos].eq_ignore_ascii_case(b"week");
            if week && !["next", "last", "previous", "this"].contains(&word) {
                continue;
            }
            let action = Action::Relative {
                amount,
                unit,
                behavior,
                reset: true,
                week,
                of: false,
            };
            offer(best, if week { 38 } else { 39 }, c, action);
            if matches!(unit, Unit::Weekday(_)) && c.space().is_some() && c.word(b"of").is_some() {
                offer(
                    best,
                    9,
                    c,
                    Action::Relative {
                        amount,
                        unit,
                        behavior,
                        reset: true,
                        week: false,
                        of: true,
                    },
                );
            }
        }
    }
    let mut c = Cursor::new(input);
    let mut sign = 1;
    while let Some(byte) = c.one(b"+-") {
        if byte == b'-' {
            sign = -sign;
        }
    }
    c.skip(b" \t");
    if let Some(amount) = c.number(1, 13, 9_999_999_999_999) {
        c.space();
        if let Some(unit) = unit(&mut c, false) {
            offer(
                best,
                44,
                c,
                Action::Relative {
                    amount: amount * sign,
                    unit,
                    behavior: WeekdayBehavior::OnOrAfter,
                    reset: false,
                    week: false,
                    of: false,
                },
            );
        }
    }
    let mut c = Cursor::new(input);
    if c.literal(b"@").is_some() {
        c.literal(b"-");
        let start = c.pos;
        while c.peek().is_some_and(|b| b.is_ascii_digit()) {
            c.pos += 1;
        }
        if c.pos > start {
            offer(best, 5, c, Action::Epoch);
            if c.literal(b".").is_some() {
                let start = c.pos;
                while c.pos - start < 6 && c.peek().is_some_and(|b| b.is_ascii_digit()) {
                    c.pos += 1;
                }
                offer(best, 6, c, Action::Epoch);
            }
        }
    }
}

fn unit(c: &mut Cursor<'_>, days_only: bool) -> Option<Unit> {
    let mut best = None;
    for (index, word) in [
        "sunday",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
    ]
    .iter()
    .enumerate()
    {
        for suffix in [0, 1, 2] {
            let mut trial = *c;
            if trial
                .word(if suffix == 0 {
                    &word.as_bytes()[..3]
                } else {
                    word.as_bytes()
                })
                .is_none()
            {
                continue;
            }
            if suffix == 2 && trial.word(b"s").is_none() {
                continue;
            }
            if best.is_none_or(|(end, _)| trial.pos > end) {
                best = Some((trial.pos, Unit::Weekday(index as i64)));
            }
        }
    }
    for word in ["weekday", "weekdays"] {
        let mut trial = *c;
        if trial.word(word.as_bytes()).is_some() && best.is_none_or(|(end, _)| trial.pos > end) {
            best = Some((trial.pos, Unit::Weekdays));
        }
    }
    if !days_only {
        for (word, field, multiplier) in [
            ("ms", 6, 1000),
            ("msec", 6, 1000),
            ("millisecond", 6, 1000),
            ("µs", 6, 1),
            ("µsec", 6, 1),
            ("microsecond", 6, 1),
            ("usec", 6, 1),
            ("sec", 5, 1),
            ("second", 5, 1),
            ("min", 4, 1),
            ("minute", 4, 1),
            ("hour", 3, 1),
            ("day", 2, 1),
            ("week", 2, 7),
            ("fortnight", 2, 14),
            ("forthnight", 2, 14),
            ("month", 1, 1),
            ("year", 0, 1),
        ] {
            let mut trial = *c;
            if trial.word(word.as_bytes()).is_some() {
                if !["ms", "µs"].contains(&word) {
                    trial.word(b"s");
                }
                if best.is_none_or(|(end, _)| trial.pos > end) {
                    best = Some((trial.pos, Unit::Field(field, multiplier)));
                }
            }
        }
    }
    let (end, unit) = best?;
    c.pos = end;
    Some(unit)
}
