//! Textual fields accept either full or abbreviated names: `M` can parse `January`.
use super::result::Parsed;
use crate::format::{DAYS, MONTHS};

pub fn parse(token: u8, input: &[u8], position: &mut usize, parsed: &mut Parsed) -> bool {
    let start = *position;
    match token {
        b'M' | b'F' | b'D' | b'l' => {
            while input.get(*position).is_some_and(|byte| {
                if matches!(token, b'D' | b'l') {
                    !b" ,\t;:/.-()".contains(byte)
                } else {
                    byte.is_ascii_alphabetic()
                }
            }) {
                *position += 1;
            }
            let word = &input[start..*position];
            if matches!(token, b'M' | b'F') {
                let month = MONTHS
                    .iter()
                    .position(|name| {
                        word.eq_ignore_ascii_case(name.as_bytes())
                            || word.eq_ignore_ascii_case(&name.as_bytes()[..3])
                    })
                    .or_else(|| word.eq_ignore_ascii_case(b"sept").then_some(8))
                    .or_else(|| {
                        [
                            "i", "ii", "iii", "iv", "v", "vi", "vii", "viii", "ix", "x", "xi",
                            "xii",
                        ]
                        .iter()
                        .position(|roman| word.eq_ignore_ascii_case(roman.as_bytes()))
                    });
                if let Some(month) = month {
                    parsed.fields[1] = Some(month as i64 + 1);
                } else {
                    parsed.error(start, "A textual month could not be found");
                }
            } else {
                let day = DAYS.iter().position(|name| {
                    word.eq_ignore_ascii_case(format!("{name}s").as_bytes())
                        || word.eq_ignore_ascii_case(name.as_bytes())
                        || word.eq_ignore_ascii_case(&name.as_bytes()[..3])
                });
                let day = day.map(|day| day as i64).or_else(|| {
                    match word.to_ascii_lowercase().as_slice() {
                        b"weekday" | b"weekdays" | b"day" | b"days" | b"year" | b"years"
                        | b"month" | b"months" | b"hour" | b"hours" | b"minute" | b"minutes"
                        | b"min" | b"mins" | b"second" | b"seconds" | b"sec" | b"secs" => Some(1),
                        b"week" | b"weeks" => Some(7),
                        b"fortnight" | b"fortnights" | b"forthnight" | b"forthnights" => Some(14),
                        b"ms" | b"msec" | b"msecs" | b"millisecond" | b"milliseconds" => Some(1000),
                        b"\xc2\xb5s" | b"usec" | b"usecs" | b"\xc2\xb5sec" | b"\xc2\xb5secs"
                        | b"microsecond" | b"microseconds" => Some(1),
                        _ => None,
                    }
                });
                if let Some(day) = day {
                    parsed.weekday = Some(day);
                } else {
                    parsed.error(start, "A textual day could not be found");
                }
            }
        }
        b'a' | b'A' => {
            if parsed.fields[3].is_none() {
                parsed.error(start, "Meridian can only come after an hour has been found");
            }
            while input
                .get(*position)
                .is_some_and(|byte| !b"aApP".contains(byte))
            {
                *position += 1;
            }
            let pm = matches!(input.get(*position), Some(b'p' | b'P'));
            let found = if *position == input.len() {
                false
            } else {
                *position += 1;
                if input.get(*position) == Some(&b'.') {
                    *position += 1;
                    if matches!(input.get(*position), Some(b'm' | b'M')) {
                        *position += 1;
                        if input.get(*position) == Some(&b'.') {
                            *position += 1;
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else if matches!(input.get(*position), Some(b'm' | b'M')) {
                    *position += 1;
                    true
                } else {
                    false
                }
            };
            if !found {
                parsed.error(start, "A meridian could not be found");
            } else if let Some(hour) = &mut parsed.fields[3] {
                if pm && *hour != 12 {
                    *hour += 12;
                }
                if !pm && *hour == 12 {
                    *hour = 0;
                }
            }
        }
        b'S' => {
            if let Some(suffix) = input.get(*position..*position + 2)
                && [b"nd", b"rd", b"st", b"th"]
                    .iter()
                    .any(|expected| suffix.eq_ignore_ascii_case(*expected))
            {
                *position += 2;
            }
        }
        b' ' => loop {
            if matches!(input.get(*position), Some(b' ' | b'\t')) {
                *position += 1;
            } else if input.get(*position..*position + 2) == Some(b"\xc2\xa0") {
                *position += 2;
            } else if input.get(*position..*position + 3) == Some(b"\xe2\x80\xaf") {
                *position += 3;
            } else {
                break;
            }
        },
        _ => return false,
    }
    true
}
