//! `new DateInterval('P2W3DT4H')` stores 17 days and four hours.

use crate::{calendar::Civil, normalize, parse};

#[derive(Default)]
pub struct IsoInterval {
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub fields: Option<[i64; 7]>,
    pub recurrences: Option<i64>,
}

pub fn parse_iso(input: &[u8]) -> Result<IsoInterval, ()> {
    if input.is_empty() {
        return Err(());
    }
    let trimmed = parse::trim(input);
    let input = if trimmed.is_empty() {
        &input[input.len() - 1..]
    } else {
        trimmed
    };
    let mut result = IsoInterval::default();
    let mut cursor = 0;
    while let Some(&token) = input.get(cursor) {
        let rest = &input[cursor..];
        match token {
            b' ' | b'.' | b',' | b'\t' | b'/' | b'\n' | 0 => cursor += 1,
            b'R' => {
                let digits = digit_count(&rest[1..]);
                if digits == 0 {
                    return Err(());
                }
                result.recurrences = Some(number(&rest[1..1 + digits.min(9)]));
                cursor += 1 + digits;
            }
            b'P' => {
                let fields = result.fields.get_or_insert([0; 7]);
                if let Some((width, civil)) = combined(&rest[1..], true, false) {
                    *fields = [
                        civil.year,
                        civil.month,
                        civil.day,
                        civil.hour,
                        civil.minute,
                        civil.second,
                        0,
                    ];
                    cursor += 1 + width;
                } else {
                    cursor += 1 + duration(&rest[1..], fields)?;
                }
            }
            b'0'..=b'9' => {
                let (width, date) = combined(rest, false, true)
                    .or_else(|| basic(rest))
                    .ok_or(())?;
                let timestamp = normalize::timestamp(date);
                if result.start.is_some() || result.fields.is_some() {
                    result.end = Some(timestamp);
                } else {
                    result.start = Some(timestamp);
                }
                cursor += width;
            }
            _ => return Err(()),
        }
    }
    Ok(result)
}

fn duration(input: &[u8], fields: &mut [i64; 7]) -> Result<usize, ()> {
    let mut cursor = 0;
    let mut minimum_rank = 0;
    let mut in_time = false;
    while cursor < input.len() {
        if input[cursor] == b'T' && !in_time {
            in_time = true;
            minimum_rank = 4;
            cursor += 1;
            if digit_count(&input[cursor..]) == 0 {
                return Err(());
            }
        }
        let digits = digit_count(&input[cursor..]);
        if digits == 0 {
            break;
        }
        if digits > 12 {
            return Err(());
        }
        let value = number(&input[cursor..cursor + digits]);
        let (rank, field) = match (in_time, input.get(cursor + digits)) {
            (false, Some(b'Y')) => (0, 0),
            (false, Some(b'M')) => (1, 1),
            (false, Some(b'W')) => (2, 2),
            (false, Some(b'D')) => (3, 2),
            (true, Some(b'H')) => (4, 3),
            (true, Some(b'M')) => (5, 4),
            (true, Some(b'S')) => (6, 5),
            _ => break,
        };
        if rank < minimum_rank {
            break;
        }
        minimum_rank = rank + 1;
        match rank {
            2 => fields[field] = fields[field].wrapping_add(value * 7),
            3 => fields[field] = fields[field].wrapping_add(value),
            _ => fields[field] = value,
        }
        cursor += digits + 1;
    }
    if cursor == 0 { Err(()) } else { Ok(cursor) }
}

fn digit_count(input: &[u8]) -> usize {
    input
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count()
}

fn number(input: &[u8]) -> i64 {
    input
        .iter()
        .fold(0, |value, byte| value * 10 + i64::from(byte - b'0'))
}

fn two(input: &[u8], index: usize) -> Option<i64> {
    let digits = input.get(index..index + 2)?;
    (digit_count(digits) == 2).then(|| number(digits))
}

fn combined(input: &[u8], zero_day: bool, z_suffix: bool) -> Option<(usize, Civil)> {
    if input.get(4) != Some(&b'-')
        || input.get(7) != Some(&b'-')
        || input.get(10) != Some(&b'T')
        || input.get(13) != Some(&b':')
        || input.get(16) != Some(&b':')
        || (z_suffix && input.get(19) != Some(&b'Z'))
        || digit_count(input.get(..4)?) != 4
    {
        return None;
    }
    let date = Civil {
        year: number(&input[..4]),
        month: two(input, 5)?,
        day: two(input, 8)?,
        hour: two(input, 11)?,
        minute: two(input, 14)?,
        second: two(input, 17)?,
    };
    is_valid(date, zero_day).then_some((19 + usize::from(z_suffix), date))
}

fn basic(input: &[u8]) -> Option<(usize, Civil)> {
    if input.get(8) != Some(&b'T')
        || input.get(15) != Some(&b'Z')
        || digit_count(input.get(..4)?) != 4
    {
        return None;
    }
    let date = Civil {
        year: number(&input[..4]),
        month: two(input, 4)?,
        day: two(input, 6)?,
        hour: two(input, 9)?,
        minute: two(input, 11)?,
        second: two(input, 13)?,
    };
    is_valid(date, false).then_some((16, date))
}

fn is_valid(date: Civil, allow_zero: bool) -> bool {
    let minimum = i64::from(!allow_zero);
    (minimum..=12).contains(&date.month)
        && (minimum..=31).contains(&date.day)
        && date.hour <= 24
        && date.minute < 60
        && date.second < 60
}
