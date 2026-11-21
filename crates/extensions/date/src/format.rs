//! PHP format letters are byte-oriented: `date('Y\\Y', 0)` returns `1970Y`.
//! Unknown bytes pass through unchanged, including invalid UTF-8 and NUL.

use std::io::Write;

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
        let mut output = Vec::with_capacity(format.len());
        let mut bytes = format.iter();
        while let Some(byte) = bytes.next() {
            if *byte == b'\\' {
                output.push(*bytes.next().unwrap_or(&0));
            } else if !self.write_letter(&mut output, *byte) {
                output.push(*byte);
            }
        }
        output
    }

    fn write_letter(&self, output: &mut Vec<u8>, letter: u8) -> bool {
        let c = self.civil;
        let month = MONTHS[(c.month - 1) as usize];
        let day = DAYS[c.weekday() as usize];
        let twelve_hour = if c.hour % 12 == 0 { 12 } else { c.hour % 12 };
        let sign = if c.year < 0 { "-" } else { "" };
        match letter {
            b'd' => two_digits(output, c.day),
            b'j' => append(output, format_args!("{}", c.day)),
            b'D' => output.extend_from_slice(&day.as_bytes()[..3]),
            b'l' => output.extend_from_slice(day.as_bytes()),
            b'S' => output.extend_from_slice(match c.day {
                1 | 21 | 31 => b"st",
                2 | 22 => b"nd",
                3 | 23 => b"rd",
                _ => b"th",
            }),
            b'w' => append(output, format_args!("{}", c.weekday())),
            b'N' => append(output, format_args!("{}", (c.weekday() + 6) % 7 + 1)),
            b'z' => append(output, format_args!("{}", c.yearday())),
            b'W' => append(output, format_args!("{:02}", c.iso_week().1)),
            b'o' => append(output, format_args!("{}", c.iso_week().0)),
            b'F' => output.extend_from_slice(month.as_bytes()),
            b'M' => output.extend_from_slice(&month.as_bytes()[..3]),
            b'm' => two_digits(output, c.month),
            b'n' => append(output, format_args!("{}", c.month)),
            b't' => append(output, format_args!("{}", month_days(c.year, c.month))),
            // timelib explicitly narrows the year to a C int for this letter.
            b'L' => output.push(b'0' + u8::from(leap(i64::from(c.year as i32)))),
            b'y' => append(output, format_args!("{:02}", c.year % 100)),
            b'Y' => append(output, format_args!("{sign}{:04}", c.year.unsigned_abs())),
            b'X' | b'x' => {
                let sign = if c.year < 0 {
                    "-"
                } else if letter == b'X' || c.year >= 10000 {
                    "+"
                } else {
                    ""
                };
                append(output, format_args!("{sign}{:04}", c.year.unsigned_abs()));
            }
            b'a' => output.extend_from_slice(if c.hour >= 12 { b"pm" } else { b"am" }),
            b'A' => output.extend_from_slice(if c.hour >= 12 { b"PM" } else { b"AM" }),
            b'B' => append(output, format_args!("{:03}", self.beats())),
            b'g' => append(output, format_args!("{twelve_hour}")),
            b'h' => two_digits(output, twelve_hour),
            b'G' => append(output, format_args!("{}", c.hour)),
            b'H' => two_digits(output, c.hour),
            b'i' => two_digits(output, c.minute),
            b's' => two_digits(output, c.second),
            b'u' => append(output, format_args!("{:06}", self.microsecond)),
            b'v' => append(output, format_args!("{:03}", self.microsecond / 1000)),
            b'e' => output.extend_from_slice(self.zone.as_bytes()),
            b'I' => output.push(b'0' + u8::from(self.offset.daylight)),
            b'O' => self.write_offset(output, b""),
            b'P' => self.write_offset(output, b":"),
            b'p' => {
                if self.gmt || matches!(self.offset.abbreviation.as_str(), "UTC" | "Z" | "GMT+0000")
                {
                    output.push(b'Z');
                } else {
                    self.write_offset(output, b":");
                }
            }
            b'T' => output.extend_from_slice(self.offset.abbreviation.as_bytes()),
            b'Z' => append(output, format_args!("{}", self.offset.seconds)),
            b'c' => {
                append(
                    output,
                    format_args!(
                        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
                        c.year, c.month, c.day, c.hour, c.minute, c.second
                    ),
                );
                self.write_offset(output, b":");
            }
            b'r' => {
                append(
                    output,
                    format_args!(
                        "{}, {:02} {} {:04} {:02}:{:02}:{:02} ",
                        &day[..3],
                        c.day,
                        &month[..3],
                        c.year,
                        c.hour,
                        c.minute,
                        c.second
                    ),
                );
                self.write_offset(output, b"");
            }
            b'U' => append(output, format_args!("{}", self.timestamp)),
            _ => return false,
        }
        true
    }

    fn write_offset(&self, output: &mut Vec<u8>, separator: &[u8]) {
        let offset = self.offset.seconds;
        output.push(if offset < 0 { b'-' } else { b'+' });
        append(output, format_args!("{:02}", (offset / 3600).abs()));
        output.extend_from_slice(separator);
        append(output, format_args!("{:02}", (offset % 3600 / 60).abs()));
    }

    pub fn beats(&self) -> i64 {
        ((self.timestamp % 86400 + 3600) * 10).rem_euclid(864000) / 864
    }
}

fn append(output: &mut Vec<u8>, text: std::fmt::Arguments<'_>) {
    let _ = output.write_fmt(text);
}

fn two_digits(output: &mut Vec<u8>, number: i64) {
    if (0..100).contains(&number) {
        let number = number as u8;
        output.extend_from_slice(&[b'0' + number / 10, b'0' + number % 10]);
    } else {
        append(output, format_args!("{number:02}"));
    }
}
