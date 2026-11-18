//! PHP accepts `4:08 pm`, `T040837`, fractional seconds and numeric timezone corrections.
use super::scan::{Cursor, offer};
use super::token::{Action, Candidate, TimeToken};

#[derive(Clone, Copy, PartialEq)]
pub enum TimeFamily {
    Hour12,
    Short12,
    Long12,
    Sql,
    Hour24,
    Short24,
    Long24,
    Compact,
}
impl TimeFamily {
    pub fn twelve(self) -> bool {
        matches!(
            self,
            Self::Hour12 | Self::Short12 | Self::Long12 | Self::Sql
        )
    }
}

pub fn candidates(input: &[u8], best: &mut Option<Candidate>) {
    for family in [
        TimeFamily::Hour12,
        TimeFamily::Short12,
        TimeFamily::Long12,
        TimeFamily::Sql,
        TimeFamily::Hour24,
        TimeFamily::Short24,
        TimeFamily::Long24,
        TimeFamily::Compact,
    ] {
        let mut cursor = Cursor::new(input);
        if let Some(time) = read(&mut cursor, family) {
            let rank = match family {
                TimeFamily::Hour12 | TimeFamily::Short12 | TimeFamily::Long12 => 10,
                TimeFamily::Sql => 11,
                TimeFamily::Compact => 14,
                _ => 12,
            };
            offer(best, rank, cursor, Action::Time(time));
        }
    }
    let mut c = Cursor::new(input);
    c.one(b"tT");
    let start = c.pos;
    if let Some(hour) = c.number(2, 2, 24)
        && let Some(minute) = c.number(2, 2, 59)
    {
        let year = input[start..c.pos]
            .iter()
            .fold(0, |n, b| n * 10 + i64::from(b - b'0'));
        offer(best, 13, c, Action::Gnu(hour, minute, year));
    }
}

pub fn read(c: &mut Cursor<'_>, family: TimeFamily) -> Option<TimeToken> {
    let twelve = family.twelve();
    if !twelve {
        c.one(b"tT");
    }
    let hour = c.number(
        if family == TimeFamily::Compact { 2 } else { 1 },
        2,
        if twelve { 12 } else { 24 },
    )?;
    if twelve && hour == 0 {
        return None;
    }
    let mut fields = [hour, 0, 0, 0];
    if family != TimeFamily::Hour12 && family != TimeFamily::Hour24 {
        if family != TimeFamily::Compact {
            c.one(if family == TimeFamily::Sql {
                b":"
            } else {
                b":."
            })?;
        }
        fields[1] = c.number(
            if matches!(
                family,
                TimeFamily::Short12 | TimeFamily::Sql | TimeFamily::Compact
            ) {
                2
            } else {
                1
            },
            2,
            59,
        )?;
    }
    if matches!(
        family,
        TimeFamily::Long12 | TimeFamily::Sql | TimeFamily::Long24 | TimeFamily::Compact
    ) {
        if family != TimeFamily::Compact {
            c.one(if family == TimeFamily::Sql {
                b":"
            } else {
                b":."
            })?;
        }
        fields[2] = c.number(
            if twelve || family == TimeFamily::Compact {
                2
            } else {
                1
            },
            2,
            60,
        )?;
    }
    if family == TimeFamily::Sql {
        fields[3] = c.fraction(b":.")?;
    }
    if family == TimeFamily::Long24 {
        let mut fraction = *c;
        if let Some(value) = fraction.fraction(b".") {
            *c = fraction;
            fields[3] = value;
        }
    }
    if twelve {
        if family != TimeFamily::Sql {
            c.space();
        }
        fields[0] = c.meridian(hour)?;
    } else if family == TimeFamily::Hour24
        && c.input
            .first()
            .is_none_or(|b| !b.eq_ignore_ascii_case(&b't'))
    {
        return None;
    }
    Some(TimeToken { fields, zone: None })
}

pub fn zone_len(input: &[u8]) -> usize {
    let mut best = 0;
    let mut c = Cursor::new(input);
    c.literal(b"(");
    let start = c.pos;
    while c.pos - start < 6 && c.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
        c.pos += 1;
    }
    if c.pos > start {
        c.literal(b")");
        best = c.pos;
    }
    let mut c = Cursor::new(input);
    if c.peek().is_some_and(|b| b.is_ascii_uppercase()) {
        c.pos += 1;
        let start = c.pos;
        while c.peek().is_some_and(|b| b.is_ascii_lowercase()) {
            c.pos += 1;
        }
        if c.pos > start {
            while c.one(b"_/-").is_some() {
                let start = c.pos;
                while c.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
                    c.pos += 1;
                }
                if c.pos == start {
                    break;
                }
                best = best.max(c.pos);
            }
        }
    }
    best.max(offset_len(input))
}

pub fn offset_len(input: &[u8]) -> usize {
    let mut c = Cursor::new(input);
    c.literal(b"GMT");
    if c.one(b"+-").is_none() {
        return 0;
    }
    let start = c;
    let mut best = 0;
    // Try both hour widths: +123 is 1:23, while +1234 is 12:34.
    for width in 1..=2 {
        let mut c = start;
        if c.number(width, width, 24).is_some() {
            best = best.max(c.pos);
            let mut minutes = c;
            minutes.literal(b":");
            if minutes.number(1, 2, 59).is_some() {
                best = best.max(minutes.pos);
            }
        }
    }
    for colon in [false, true] {
        let mut c = start;
        let result = (|| {
            c.number(2, 2, 24)?;
            if colon {
                c.literal(b":")?;
            }
            c.number(2, 2, 59)?;
            if colon {
                c.literal(b":")?;
            }
            c.number(2, 2, 60)
        })();
        if result.is_some() {
            best = best.max(c.pos);
        }
    }
    best
}
