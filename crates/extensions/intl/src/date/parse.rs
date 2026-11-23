//! Parsing reads UTS #35 date fields and updates a by-reference UTF-8 byte offset.
//!
//! ```php
//! $f = new IntlDateFormatter('en_US', -1, -1, 'UTC', 1, 'yyyy-MM-dd');
//! $f->parse('2024-01-02', $offset); // 1704153600, offset 10
//! ```

use phera_ext_date::{
    ZoneKind,
    calendar::{Civil, month_days},
};
use phera_extension_api::{ArrayKey, Context, NativeError, NativeResult, PhpArray, Value, args};

use super::{Formatter, fail, historical, render::Renderer};
use crate::number::digit;

pub fn parse(
    context: &mut dyn Context,
    state: &mut Formatter,
    arguments: &[Value],
    site: &str,
    localtime: bool,
) -> NativeResult {
    let offset = args::reference(arguments, 1);
    let start = offset
        .and_then(|cell| {
            let value = cell.try_borrow().ok()?;
            Some(match &*value {
                Value::Int(number) => *number,
                Value::Float(number) => *number as i64,
                Value::String(text) => std::str::from_utf8(text.as_bytes())
                    .ok()
                    .and_then(|text| text.parse::<i64>().ok())
                    .unwrap_or(0),
                _ => 0,
            })
        })
        .unwrap_or(0);
    let Ok(start) = i32::try_from(start) else {
        return crate::error::fail(
            context,
            site,
            1,
            "String index is out of valid range.",
            Value::Bool(false),
        );
    };
    let source = args::bytes(arguments, 0);
    if start < 0 || start as usize > source.len() {
        return Ok(Value::Bool(false));
    }
    let Ok(text) = std::str::from_utf8(source) else {
        return fail(
            context,
            state,
            site,
            10,
            "Error converting timezone to UTF-16",
        );
    };
    let start = start as usize;
    let Some(input) = text.get(start..) else {
        return fail(context, state, site, 10, "Invalid UTF-8 offset");
    };
    let (parsed, length) = fields(state, input);
    if let Some(offset) = offset {
        *offset.try_borrow_mut().map_err(|_| {
            NativeError::error("IntlDateFormatter offset is already being modified")
        })? = Value::Int((start + length) as i64);
    }
    let Some(Parsed { civil, zone, .. }) = parsed else {
        return fail(context, state, site, 9, "Date parsing failed");
    };
    let timestamp = historical::resolve(&zone, civil);
    if !localtime {
        return Ok(Value::Int(timestamp));
    }
    let shift = zone.offset(timestamp);
    let shifted = Civil::from_timestamp(timestamp.saturating_add(i64::from(shift.seconds)));
    let (shifted, weekday, yearday) = historical::fields(shifted);
    let rows = [
        ("tm_sec", shifted.second),
        ("tm_min", shifted.minute),
        ("tm_hour", shifted.hour),
        ("tm_year", shifted.year - 1900),
        ("tm_mday", shifted.day),
        ("tm_wday", weekday),
        ("tm_yday", yearday),
        ("tm_mon", shifted.month - 1),
        ("tm_isdst", i64::from(shift.daylight)),
    ];
    let value: PhpArray = rows
        .into_iter()
        .map(|(key, value)| (ArrayKey::str(key), Value::Int(value)))
        .collect();
    Ok(Value::from(value))
}

pub(super) struct Parsed {
    pub civil: Civil,
    pub zone: ZoneKind,
    pub micros: i64,
}

pub(super) fn fields(state: &Formatter, text: &str) -> (Option<Parsed>, usize) {
    let mut scanner = Scanner {
        rest: text,
        state,
        meridiem: None,
        before_era: false,
        zone: state.zone.clone(),
        civil: Civil::from_timestamp(0),
        micros: 0,
    };
    let mut pattern = state.pattern.chars().peekable();
    let mut quoted = false;
    while let Some(symbol) = pattern.next() {
        let valid = if symbol == '\'' {
            if pattern.next_if_eq(&'\'').is_some() {
                scanner.literal('\'')
            } else {
                quoted = !quoted;
                true
            }
        } else if !quoted && symbol.is_ascii_alphabetic() {
            let mut width = 1;
            while pattern.next_if_eq(&symbol).is_some() {
                width += 1;
            }
            scanner.field(symbol, width)
        } else {
            scanner.literal(symbol)
        };
        if !valid {
            let consumed = text.len() - scanner.rest.len();
            let attempted = if consumed == 0 && state.lenient && text.starts_with('-') {
                text.bytes()
                    .take_while(|byte| byte.is_ascii_digit() || matches!(byte, b'-' | b':' | b' '))
                    .count()
            } else {
                consumed
            };
            return (None, attempted);
        }
    }
    if let Some(pm) = scanner.meridiem {
        scanner.civil.hour = scanner.civil.hour.rem_euclid(12) + i64::from(pm) * 12;
    }
    if scanner.before_era {
        scanner.civil.year = 1 - scanner.civil.year;
    }
    let civil = scanner.civil;
    if !state.lenient
        && (!(1..=12).contains(&civil.month)
            || !(1..=month_days(civil.year, civil.month)).contains(&civil.day)
            || !(0..24).contains(&civil.hour)
            || !(0..60).contains(&civil.minute)
            || !(0..60).contains(&civil.second))
    {
        return (None, text.len() - scanner.rest.len());
    }
    (
        Some(Parsed {
            civil,
            zone: scanner.zone,
            micros: scanner.micros,
        }),
        text.len() - scanner.rest.len(),
    )
}

struct Scanner<'a> {
    rest: &'a str,
    state: &'a Formatter,
    meridiem: Option<bool>,
    before_era: bool,
    zone: ZoneKind,
    civil: Civil,
    micros: i64,
}

impl Scanner<'_> {
    fn literal(&mut self, literal: char) -> bool {
        if let Some(rest) = self.rest.strip_prefix(literal) {
            self.rest = rest;
            true
        } else if self.state.lenient
            && literal.is_whitespace()
            && self.rest.starts_with(char::is_whitespace)
        {
            self.rest = self.rest.trim_start();
            true
        } else {
            false
        }
    }

    fn field(&mut self, symbol: char, width: usize) -> bool {
        let number = match symbol {
            'G' => {
                let era = self.name(symbol, width, 2, |before| Civil {
                    year: if before == 0 { 2000 } else { -1000 },
                    ..Civil::from_timestamp(0)
                });
                self.before_era = era == Some(1);
                return era.is_some();
            }
            'M' | 'L' if width >= 3 => self
                .name(symbol, width, 12, |month| Civil {
                    year: 2000,
                    month: i64::from(month) + 1,
                    ..Civil::from_timestamp(0)
                })
                .map(|month| i64::from(month) + 1),
            'E' | 'c' if width >= 3 => {
                return self
                    .name(symbol, width, 7, |day| Civil {
                        year: 2000,
                        month: 1,
                        day: i64::from(day) + 1,
                        ..Civil::from_timestamp(0)
                    })
                    .is_some();
            }
            'a' | 'b' | 'B' => {
                self.meridiem = self
                    .name(symbol, width, 2, |part| Civil {
                        hour: if part == 0 { 9 } else { 21 },
                        ..Civil::from_timestamp(0)
                    })
                    .map(|part| part == 1);
                return self.meridiem.is_some();
            }
            'X' | 'x' | 'Z' | 'O' | 'z' | 'v' | 'V' => return self.timezone(),
            'y' | 'u' | 'Y' => self.numeric(if width == 2 { 2 } else { width.clamp(4, 10) }),
            'D' => self.numeric(3),
            'S' => return self.fraction(width),
            _ => self.numeric(2),
        };
        let Some(number) = number else {
            return false;
        };
        match symbol {
            'y' | 'u' | 'Y' => {
                self.civil.year = if width == 2 && number < 100 {
                    if number >= 46 {
                        1900 + number
                    } else {
                        2000 + number
                    }
                } else {
                    number
                };
            }
            'M' | 'L' => self.civil.month = number,
            'd' => self.civil.day = number,
            'D' => {
                self.civil.month = 1;
                self.civil.day = number;
            }
            'H' | 'h' | 'K' => self.civil.hour = number,
            'k' => self.civil.hour = if number == 24 { 0 } else { number },
            'm' => self.civil.minute = number,
            's' => self.civil.second = number,
            'e' => {}
            _ => return false,
        }
        true
    }

    fn fraction(&mut self, width: usize) -> bool {
        let remaining = self.rest;
        let Some(fraction) = self.numeric(width.min(9)) else {
            return false;
        };
        let bytes = remaining.len() - self.rest.len();
        let digits = remaining[..bytes].chars().count() as u32;
        self.micros = if digits > 6 {
            fraction / 10_i64.pow(digits - 6)
        } else {
            fraction * 10_i64.pow(6 - digits)
        };
        true
    }

    fn numeric(&mut self, max_digits: usize) -> Option<i64> {
        let mut integer = 0;
        let mut bytes = 0;
        for scalar in self.rest.chars().take(max_digits) {
            let Some(value) = digit(scalar) else {
                break;
            };
            integer = integer * 10 + i64::from(value);
            bytes += scalar.len_utf8();
        }
        if bytes == 0 {
            return None;
        }
        self.rest = &self.rest[bytes..];
        Some(integer)
    }

    fn name(
        &mut self,
        symbol: char,
        width: usize,
        count: u8,
        sample: impl Fn(u8) -> Civil,
    ) -> Option<u8> {
        let pattern: String = std::iter::repeat_n(symbol, width).collect();
        let renderer = Renderer::new(&self.state.locale, &pattern, self.state.calendar)?;
        let mut best = None;
        let mut length = 0;
        for index in 0..count {
            let seconds = i64::try_from(sample(index).timestamp()).ok()?;
            let name = renderer.format(seconds, 0, &ZoneKind::Fixed(0))?;
            if let Some(part) = self.rest.get(..name.len())
                && (part == name || part.to_lowercase() == name.to_lowercase())
                && name.len() > length
            {
                length = name.len();
                best = Some(index);
            }
        }
        self.rest = &self.rest[length..];
        best
    }

    fn timezone(&mut self) -> bool {
        for text in ["GMT", "UTC", "Z"] {
            if let Some(rest) = self.rest.strip_prefix(text) {
                self.rest = rest;
                self.zone = ZoneKind::Fixed(0);
                if !rest.starts_with(['+', '-']) {
                    return true;
                }
                break;
            }
        }
        let length = self
            .rest
            .bytes()
            .take_while(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b':' | b'/' | b'_')
            })
            .count();
        let Some(timezone) = phera_ext_date::parse_timezone(&self.rest.as_bytes()[..length]) else {
            return false;
        };
        self.rest = &self.rest[length..];
        self.zone = timezone;
        true
    }
}
