//! Numeric and textual date families: `10/11/12` is American, `10.11.12` is European.
use super::scan::{Cursor, offer};
use super::token::{Action, Candidate, DateToken};

#[derive(Clone, Copy, PartialEq)]
pub enum DateFamily {
    AmericanShort,
    American,
    Iso,
    Slash,
    IsoShort,
    Expanded,
    YearMonth,
    Gnu,
    FullText,
    Pointed,
    PointedShort,
    MonthYear,
    YearText,
    Textual,
    MonthDay,
    DayMonth,
    Compact,
    Ordinal,
}
impl DateFamily {
    fn rank(self) -> usize {
        match self {
            Self::AmericanShort => 15,
            Self::American => 15,
            Self::Iso => 16,
            Self::Slash => 16,
            Self::IsoShort => 17,
            Self::Expanded => 18,
            Self::YearMonth => 19,
            Self::Gnu => 20,
            Self::FullText => 21,
            Self::Pointed => 22,
            Self::PointedShort => 23,
            Self::MonthYear => 24,
            Self::YearText => 25,
            Self::Textual => 26,
            Self::MonthDay => 26,
            Self::DayMonth => 27,
            Self::Compact => 28,
            Self::Ordinal => 30,
        }
    }
}

pub fn candidates(input: &[u8], best: &mut Option<Candidate>) {
    for family in [
        DateFamily::AmericanShort,
        DateFamily::American,
        DateFamily::Iso,
        DateFamily::Slash,
        DateFamily::IsoShort,
        DateFamily::Expanded,
        DateFamily::YearMonth,
        DateFamily::Gnu,
        DateFamily::FullText,
        DateFamily::Pointed,
        DateFamily::PointedShort,
        DateFamily::MonthYear,
        DateFamily::YearText,
        DateFamily::Textual,
        DateFamily::MonthDay,
        DateFamily::DayMonth,
        DateFamily::Compact,
        DateFamily::Ordinal,
    ] {
        let mut c = Cursor::new(input);
        if let Some(date) = read(&mut c, family) {
            offer(best, family.rank(), c, Action::Date(date));
        }
    }
    let mut c = Cursor::new(input);
    if let Some(year) = c.year(4, 4, false) {
        offer(best, 35, c, Action::Year(year));
    }
    let mut c = Cursor::new(input);
    if let Some(month) = c.month(false, false) {
        offer(best, 40, c, Action::Date(DateToken::new(None, month, None)));
    }
    for day in [false, true] {
        let mut c = Cursor::new(input);
        let result = (|| {
            let year = c.year(4, 4, false)?;
            c.literal(b"-");
            c.literal(b"W")?;
            let week = c.number(2, 2, 53)?;
            if week == 0 {
                return None;
            }
            let weekday = if day {
                c.literal(b"-");
                c.number(1, 1, 7)?
            } else {
                1
            };
            Some(Action::Week(year, week, weekday))
        })();
        if let Some(action) = result {
            offer(best, if day { 31 } else { 32 }, c, action);
        }
    }
    for reverse in [false, true] {
        let mut c = Cursor::new(input);
        let result = (|| {
            let first = if reverse {
                c.year(1, 4, true)?
            } else {
                c.month(true, false)?
            };
            c.literal(b"-")?;
            let second = if reverse {
                c.month(true, false)?
            } else {
                c.day(true)?
            };
            c.literal(b"-")?;
            let third = if reverse {
                c.day(true)?
            } else {
                c.year(1, 4, true)?
            };
            Some(if reverse {
                DateToken::new(Some(first), second, Some(third))
            } else {
                DateToken::new(Some(third), first, Some(second))
            })
        })();
        if let Some(date) = result {
            offer(best, if reverse { 34 } else { 33 }, c, Action::Date(date));
        }
    }
}

pub fn read(c: &mut Cursor<'_>, family: DateFamily) -> Option<DateToken> {
    let mut overflow = false;
    let (year, month, day) = match family {
        DateFamily::AmericanShort | DateFamily::American => {
            let month = c.number(1, 2, 12)?;
            c.literal(b"/")?;
            let day = c.day(false)?;
            let year = if family == DateFamily::American {
                c.literal(b"/")?;
                Some(c.year(1, 4, true)?)
            } else {
                None
            };
            (year, month, day)
        }
        DateFamily::Iso | DateFamily::Slash | DateFamily::IsoShort | DateFamily::Expanded => {
            let sign = if family == DateFamily::Iso || family == DateFamily::Expanded {
                c.one(b"+-")
            } else {
                None
            };
            let start = c.pos;
            let year = if family == DateFamily::Expanded {
                sign?;
                while c.pos - start < 19 && c.peek().is_some_and(|b| b.is_ascii_digit()) {
                    c.pos += 1;
                }
                if c.pos - start < 5 {
                    return None;
                }
                let bytes = &c.input[start..c.pos];
                let magnitude = bytes.iter().fold(0u64, |n, b| n * 10 + u64::from(b - b'0'));
                let signed = i128::from(magnitude) * if sign == Some(b'-') { -1 } else { 1 };
                overflow = i64::try_from(signed).is_err();
                i64::try_from(signed).unwrap_or(0)
            } else {
                c.year(
                    if family == DateFamily::IsoShort { 2 } else { 4 },
                    if family == DateFamily::IsoShort { 2 } else { 4 },
                    family == DateFamily::IsoShort,
                )? * if sign == Some(b'-') { -1 } else { 1 }
            };
            c.literal(if family == DateFamily::Slash {
                b"/"
            } else {
                b"-"
            })?;
            let month = c.number(if family == DateFamily::Slash { 1 } else { 2 }, 2, 12)?;
            c.literal(if family == DateFamily::Slash {
                b"/"
            } else {
                b"-"
            })?;
            let day = c.day(family != DateFamily::Slash)?;
            if family == DateFamily::Slash && c.pos - start == 10 {
                c.literal(b"/");
            }
            (Some(year), month, day)
        }
        DateFamily::YearMonth | DateFamily::Gnu => {
            let year = c.year(
                if family == DateFamily::YearMonth {
                    4
                } else {
                    1
                },
                4,
                true,
            )?;
            c.literal(b"-")?;
            let month = c.number(1, 2, 12)?;
            let day = if family == DateFamily::YearMonth {
                1
            } else {
                c.literal(b"-")?;
                c.day(false)?
            };
            (Some(year), month, day)
        }
        DateFamily::FullText => {
            let day = c.day(false)?;
            c.skip(b" \t.-");
            let month = c.month(false, true)?;
            c.skip(b" \t.-");
            (Some(c.year(1, 4, true)?), month, day)
        }
        DateFamily::Pointed | DateFamily::PointedShort => {
            let day = c.day(false)?;
            c.one(if family == DateFamily::Pointed {
                b".\t-"
            } else {
                b".\t"
            })?;
            let month = c.number(1, 2, 12)?;
            c.one(if family == DateFamily::Pointed {
                b".-"
            } else {
                b"."
            })?;
            let width = if family == DateFamily::Pointed { 4 } else { 2 };
            (
                Some(c.year(width, width, family == DateFamily::PointedShort)?),
                month,
                day,
            )
        }
        DateFamily::MonthYear | DateFamily::YearText => {
            if family == DateFamily::MonthYear {
                let month = c.month(false, true)?;
                c.skip(b" .\t-");
                (Some(c.year(4, 4, false)?), month, 1)
            } else {
                let year = c.year(4, 4, false)?;
                c.skip(b" .\t-");
                (Some(year), c.month(false, true)?, 1)
            }
        }
        DateFamily::Textual | DateFamily::MonthDay => {
            let month = c.month(false, true)?;
            c.skip(b" .\t-");
            let day = c.day(false)?;
            if c.skip(b",.stndrh\t ") == 0 {
                if family == DateFamily::Textual {
                    return None;
                }
                if c.peek().is_none() {
                    c.pos += 1;
                } else {
                    c.literal(b"\0")?;
                }
            }
            let year = if family == DateFamily::Textual {
                Some(c.year(1, 4, true)?)
            } else {
                None
            };
            return Some(DateToken {
                fields: [year, Some(month), Some(day)],
                replace_year: true,
                overflow: false,
            });
        }
        DateFamily::DayMonth => {
            let day = c.day(false)?;
            c.skip(b" .\t-");
            (None, c.month(false, true)?, day)
        }
        DateFamily::Compact => (
            Some(c.year(4, 4, false)?),
            c.number(2, 2, 12)?,
            c.day(true)?,
        ),
        DateFamily::Ordinal => {
            let year = c.year(4, 4, false)?;
            c.one(b".-");
            let ordinal = c.number(3, 3, 366)?;
            if ordinal == 0 {
                return None;
            }
            (Some(year), 1, ordinal)
        }
    };
    let mut date = DateToken::new(year, month, Some(day));
    date.overflow = overflow;
    Some(date)
}
