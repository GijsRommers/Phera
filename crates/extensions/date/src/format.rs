//! PHP format letters are byte-oriented: `date('Y\\Y', 0)` returns `1970Y`.
//! Unknown bytes pass through unchanged, including invalid UTF-8 and NUL.

use crate::calendar::{Civil, leap, month_days};
use crate::zone::{Offset, Zone};

pub const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
pub const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub struct DateFormat {
    pub civil: Civil,
    pub offset: Offset,
    pub zone: String,
    pub timestamp: i64,
    pub microsecond: u32,
    pub gmt: bool,
}

impl DateFormat {
    pub fn new(timestamp: i64, zone: &Zone, gmt: bool) -> Self {
        let (civil, mut offset) = zone.local(timestamp);
        if gmt {
            offset.abbreviation = "GMT".into();
        }
        Self {
            civil,
            offset,
            zone: zone.name.clone(),
            timestamp,
            microsecond: 0,
            gmt,
        }
    }

    pub fn render(&self, format: &[u8]) -> Vec<u8> {
        let mut output = Vec::new();
        let mut bytes = format.iter();
        while let Some(byte) = bytes.next() {
            if *byte == b'\\' {
                output.push(*bytes.next().unwrap_or(&0));
            } else if let Some(text) = self.letter(*byte) {
                output.extend_from_slice(text.as_bytes());
            } else {
                output.push(*byte);
            }
        }
        output
    }

    pub fn letter(&self, letter: u8) -> Option<String> {
        let c = self.civil;
        let month = MONTHS[(c.month - 1) as usize];
        let day = DAYS[c.weekday() as usize];
        let twelve_hour = if c.hour % 12 == 0 { 12 } else { c.hour % 12 };
        let sign = if c.year < 0 { "-" } else { "" };
        Some(match letter {
            b'd' => format!("{:02}", c.day),
            b'j' => c.day.to_string(),
            b'D' => day[..3].into(),
            b'l' => day.into(),
            b'S' => match c.day {
                1 | 21 | 31 => "st",
                2 | 22 => "nd",
                3 | 23 => "rd",
                _ => "th",
            }
            .into(),
            b'w' => c.weekday().to_string(),
            b'N' => ((c.weekday() + 6) % 7 + 1).to_string(),
            b'z' => c.yearday().to_string(),
            b'W' => format!("{:02}", c.iso_week().1),
            b'o' => c.iso_week().0.to_string(),
            b'F' => month.into(),
            b'M' => month[..3].into(),
            b'm' => format!("{:02}", c.month),
            b'n' => c.month.to_string(),
            b't' => month_days(c.year, c.month).to_string(),
            // timelib explicitly narrows the year to a C int for this letter.
            b'L' => u8::from(leap(i64::from(c.year as i32))).to_string(),
            b'y' => format!("{:02}", c.year % 100),
            b'Y' => format!("{sign}{:04}", c.year.unsigned_abs()),
            b'X' | b'x' => {
                let sign = if c.year < 0 {
                    "-"
                } else if letter == b'X' || c.year >= 10000 {
                    "+"
                } else {
                    ""
                };
                format!("{sign}{:04}", c.year.unsigned_abs())
            }
            b'a' => if c.hour >= 12 { "pm" } else { "am" }.into(),
            b'A' => if c.hour >= 12 { "PM" } else { "AM" }.into(),
            b'B' => format!("{:03}", self.beats()),
            b'g' => twelve_hour.to_string(),
            b'h' => format!("{twelve_hour:02}"),
            b'G' => c.hour.to_string(),
            b'H' => format!("{:02}", c.hour),
            b'i' => format!("{:02}", c.minute),
            b's' => format!("{:02}", c.second),
            b'u' => format!("{:06}", self.microsecond),
            b'v' => format!("{:03}", self.microsecond / 1000),
            b'e' => self.zone.clone(),
            b'I' => u8::from(self.offset.daylight).to_string(),
            b'O' => self.numeric_offset(false),
            b'P' => self.numeric_offset(true),
            b'p' => {
                if self.gmt || matches!(self.offset.abbreviation.as_str(), "UTC" | "Z" | "GMT+0000")
                {
                    "Z".into()
                } else {
                    self.numeric_offset(true)
                }
            }
            b'T' => self.offset.abbreviation.clone(),
            b'Z' => self.offset.seconds.to_string(),
            b'c' => format!(
                "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}",
                c.year,
                c.month,
                c.day,
                c.hour,
                c.minute,
                c.second,
                self.numeric_offset(true)
            ),
            b'r' => format!(
                "{}, {:02} {} {:04} {:02}:{:02}:{:02} {}",
                &day[..3],
                c.day,
                &month[..3],
                c.year,
                c.hour,
                c.minute,
                c.second,
                self.numeric_offset(false)
            ),
            b'U' => self.timestamp.to_string(),
            _ => return None,
        })
    }

    pub fn numeric_offset(&self, colon: bool) -> String {
        let offset = self.offset.seconds;
        let sign = if offset < 0 { '-' } else { '+' };
        let separator = if colon { ":" } else { "" };
        format!(
            "{sign}{:02}{separator}{:02}",
            (offset / 3600).abs(),
            (offset % 3600 / 60).abs()
        )
    }

    pub fn beats(&self) -> i64 {
        ((self.timestamp % 86400 + 3600) * 10).rem_euclid(864000) / 864
    }
}
