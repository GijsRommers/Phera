//! Numeric format fields consume bounded digits; `Y` accepts one to four digits.
use super::result::{Parsed, ParsedZone};
use crate::calendar::Civil;

pub fn parse(token: u8, input: &[u8], position: &mut usize, parsed: &mut Parsed) -> bool {
    let (limit, message) = match token {
        b'd' | b'j' => (2, "A two digit day could not be found"),
        b'm' | b'n' => (2, "A two digit month could not be found"),
        b'y' => (2, "A two digit year could not be found"),
        b'Y' => (4, "A four digit year could not be found"),
        b'x' | b'X' => (19, "An expanded digit year could not be found"),
        b'h' | b'g' | b'H' | b'G' => (2, "A two digit hour could not be found"),
        b'i' => (2, "A two digit minute could not be found"),
        b's' => (2, "A two digit second could not be found"),
        b'u' => (6, "A six digit microsecond could not be found"),
        b'v' => (3, "A three digit millisecond could not be found"),
        b'z' => (3, "A three digit day-of-year could not be found"),
        b'U' => (24, ""),
        _ => return false,
    };
    let start = *position;
    let signed = matches!(token, b'x' | b'X' | b'U');
    if input
        .get(*position)
        .is_some_and(|byte| !byte.is_ascii_digit() && !(signed && matches!(byte, b'+' | b'-')))
    {
        parsed.error(start, "Unexpected data found.");
    }
    if token == b'z' && parsed.fields[0].is_none() {
        parsed.error(
            start,
            "A 'day of year' can only come after a year has been found",
        );
    }
    let result = read_number(input, position, limit, signed, parsed);
    let Some((value, length)) = result else {
        let field = match token {
            b'Y' | b'y' | b'x' | b'X' => Some(0),
            b'm' | b'n' => Some(1),
            b'd' | b'j' => Some(2),
            b'h' | b'g' | b'H' | b'G' => Some(3),
            _ => None,
        };
        if let Some(field) = field {
            parsed.fields[field] = None;
        }
        parsed.error(start, message);
        return true;
    };
    if matches!(token, b'i' | b's') && length != 2 {
        parsed.error(start, message);
        return true;
    }
    match token {
        b'd' | b'j' => parsed.fields[2] = Some(value),
        b'm' | b'n' => parsed.fields[1] = Some(value),
        b'Y' | b'x' | b'X' => parsed.fields[0] = Some(value),
        b'y' => parsed.fields[0] = Some(value + if value < 70 { 2000 } else { 1900 }),
        b'h' | b'g' | b'H' | b'G' => {
            parsed.fields[3] = Some(value);
            if matches!(token, b'h' | b'g') && value > 12 {
                parsed.error(start, "Hour cannot be higher than 12");
            }
        }
        b'i' => parsed.fields[4] = Some(value),
        b's' => parsed.fields[5] = Some(value),
        b'u' | b'v' => {
            parsed.microsecond =
                Some((value as f64 * 10_f64.powi(6 - (*position - start) as i32)) as i64)
        }
        b'z' => {
            if let Some(year) = parsed.fields[0] {
                let civil = Civil {
                    year,
                    month: 1,
                    day: value + 1,
                    hour: 0,
                    minute: 0,
                    second: 0,
                };
                let normalized = Civil::from_timestamp(crate::normalize::timestamp(civil));
                parsed.fields[..3].copy_from_slice(&[
                    Some(normalized.year),
                    Some(normalized.month),
                    Some(normalized.day),
                ]);
            }
        }
        b'U' => {
            let civil = Civil::from_timestamp(value);
            parsed.fields = [
                Some(civil.year),
                Some(civil.month),
                Some(civil.day),
                Some(civil.hour),
                Some(civil.minute),
                Some(civil.second),
            ];
            parsed.microsecond = Some(0);
            parsed.zone = Some(ParsedZone::Offset(0));
        }
        _ => {}
    }
    true
}

fn read_number(
    input: &[u8],
    position: &mut usize,
    limit: usize,
    signed: bool,
    parsed: &mut Parsed,
) -> Option<(i64, usize)> {
    while input
        .get(*position)
        .is_some_and(|byte| !byte.is_ascii_digit() && !(signed && matches!(byte, b'+' | b'-')))
    {
        *position += 1;
    }
    let mut negative = false;
    if signed {
        while let Some(byte @ (b'+' | b'-')) = input.get(*position) {
            negative ^= *byte == b'-';
            *position += 1;
        }
        while input
            .get(*position)
            .is_some_and(|byte| !byte.is_ascii_digit())
        {
            *position += 1;
        }
    }
    let begin = *position;
    let mut value = 0_i128;
    while *position - begin < limit && input.get(*position).is_some_and(u8::is_ascii_digit) {
        value = value * 10 + i128::from(input[*position] - b'0');
        *position += 1;
    }
    let length = *position - begin;
    if signed && length == 0 {
        parsed.error(0, "Found unexpected data");
        return Some((0, 0));
    }
    if length == 0 {
        return None;
    }
    if negative {
        value = -value;
    }
    match i64::try_from(value) {
        Ok(value) => Some((value, length)),
        Err(_) => {
            parsed.error(0, "Number out of range");
            Some((0, length))
        }
    }
}
