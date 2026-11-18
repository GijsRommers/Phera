//! Compound formats win longest matches, e.g. `May 2 12:30` does not parse 12 as its year.
use super::scan::{Cursor, offer};
use super::token::{Action, Candidate, DateToken, TimeToken};
use super::{dates, time};

#[derive(Clone, Copy, PartialEq)]
enum CompoundFamily {
    Soap,
    XmlRpc,
    Compact,
    Wddx,
    Exif,
    NarrowCompact,
}

pub fn candidates(input: &[u8], best: &mut Option<Candidate>) {
    for family in [
        CompoundFamily::Soap,
        CompoundFamily::XmlRpc,
        CompoundFamily::Compact,
        CompoundFamily::Wddx,
        CompoundFamily::Exif,
        CompoundFamily::NarrowCompact,
    ] {
        let mut c = Cursor::new(input);
        if let Some((date, time)) = read(&mut c, family) {
            offer(best, 29, c, Action::Compound(date, time, true));
        }
    }
    let mut c = Cursor::new(input);
    let clf = (|| {
        let day = c.day(false)?;
        c.literal(b"/")?;
        let month = c.month(true, false)?;
        c.literal(b"/")?;
        let year = c.year(4, 4, false)?;
        c.literal(b":")?;
        let hour = c.number(2, 2, 24)?;
        c.literal(b":")?;
        let minute = c.number(2, 2, 59)?;
        c.literal(b":")?;
        let second = c.number(2, 2, 60)?;
        c.space()?;
        let zone = c.pos;
        let len = time::offset_len(&input[zone..]);
        if len == 0 {
            return None;
        }
        c.pos += len;
        Some((
            DateToken::new(Some(year), month, Some(day)),
            TimeToken {
                fields: [hour, minute, second, 0],
                zone: Some(zone),
            },
        ))
    })();
    if let Some((date, time)) = clf {
        offer(best, 35, c, Action::Compound(date, time, true));
    }
    let mut start = Cursor::new(input);
    if let Some(mut date) = dates::read(&mut start, dates::DateFamily::MonthDay) {
        date.replace_year = false;
        for family in [
            time::TimeFamily::Short12,
            time::TimeFamily::Long12,
            time::TimeFamily::Short24,
            time::TimeFamily::Long24,
        ] {
            let mut c = start;
            if let Some(mut clock) = time::read(&mut c, family) {
                let source_start =
                    start.pos - usize::from(input.get(start.pos.wrapping_sub(1)) == Some(&0));
                let parsed = super::short_time::read(
                    &input[..c.pos.min(input.len())],
                    source_start,
                    family.twelve(),
                );
                offer(best, 42, c, Action::Compound(date, parsed, false));
                if family == time::TimeFamily::Long24
                    && clock.fields[3] == 0
                    && c.pos <= input.len()
                    && input[..c.pos]
                        .iter()
                        .rposition(|b| b":.".contains(b))
                        .is_some_and(|p| c.pos - p - 1 == 2)
                {
                    c.space();
                    let zone = c.pos;
                    let len = time::zone_len(&input[zone..]);
                    if len > 0 {
                        clock.zone = Some(zone);
                        c.pos += len;
                        offer(best, 43, c, Action::Compound(date, clock, false));
                    }
                }
            }
        }
    }
}

fn read(c: &mut Cursor<'_>, family: CompoundFamily) -> Option<(DateToken, TimeToken)> {
    let narrow_hour = family == CompoundFamily::NarrowCompact;
    let family = if narrow_hour {
        CompoundFamily::Compact
    } else {
        family
    };
    let year = c.year(4, 4, false)?;
    if family == CompoundFamily::Soap || family == CompoundFamily::Wddx {
        c.literal(b"-")?;
    }
    if family == CompoundFamily::Exif {
        c.literal(b":")?;
    }
    let month = c.number(if family == CompoundFamily::Wddx { 1 } else { 2 }, 2, 12)?;
    if family == CompoundFamily::Soap || family == CompoundFamily::Wddx {
        c.literal(b"-")?;
    }
    if family == CompoundFamily::Exif {
        c.literal(b":")?;
    }
    let day = c.day(family != CompoundFamily::Wddx)?;
    if family == CompoundFamily::Compact {
        c.one(b"tT")?;
    } else {
        c.literal(if family == CompoundFamily::Exif {
            b" "
        } else {
            b"T"
        })?;
    }
    let clock_start = c.pos;
    let hour = c.number(
        if matches!(family, CompoundFamily::Soap | CompoundFamily::Exif) {
            2
        } else {
            1
        },
        if narrow_hour { 1 } else { 2 },
        24,
    )?;
    if family != CompoundFamily::Compact {
        c.literal(b":")?;
    }
    let minute = c.number(if family == CompoundFamily::Wddx { 1 } else { 2 }, 2, 59)?;
    if family != CompoundFamily::Compact {
        c.literal(b":")?;
    }
    let second = c.number(if family == CompoundFamily::Wddx { 1 } else { 2 }, 2, 60)?;
    let mut time = TimeToken {
        fields: [hour, minute, second, 0],
        zone: None,
    };
    if family == CompoundFamily::Compact {
        let mut numbers = Cursor::new(&c.input[clock_start..c.pos]);
        for field in &mut time.fields[..3] {
            *field = numbers.number(1, 2, 99).unwrap_or(-9999999);
        }
    }
    if family == CompoundFamily::Soap {
        time.fields[3] = c.fraction(b".")?;
        let len = time::offset_len(&c.input[c.pos..]);
        if len > 0 {
            time.zone = Some(c.pos);
            c.pos += len;
        }
    }
    Some((DateToken::new(Some(year), month, Some(day)), time))
}
