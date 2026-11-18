//! In parsed strings `EDT` is a fixed abbreviation, while `America/New_York` has rules.
//! Abbreviation priority follows timelib's table, not the host timezone database.
use super::result::ParsedZone;
use crate::zone::Zone;

pub fn parse(input: &[u8], position: &mut usize) -> (ParsedZone, bool) {
    let mut parentheses = 0;
    while let Some(byte @ (b' ' | b'\t' | b'(')) = input.get(*position) {
        parentheses += usize::from(*byte == b'(');
        *position += 1;
    }
    if input.get(*position..*position + 3) == Some(b"GMT")
        && matches!(input.get(*position + 3), Some(b'+' | b'-'))
    {
        *position += 3;
    }
    let result = if let Some(sign @ (b'+' | b'-')) = input.get(*position).copied() {
        *position += 1;
        let start = *position;
        while input
            .get(*position)
            .is_some_and(|b| b.is_ascii_digit() || *b == b':')
        {
            *position += 1;
        }
        let seconds = offset(&input[start..*position]);
        (
            ParsedZone::Offset(seconds.unwrap_or_default() * if sign == b'-' { -1 } else { 1 }),
            seconds.is_some(),
        )
    } else {
        let start = *position;
        while input
            .get(*position)
            .is_some_and(|b| b.is_ascii_alphanumeric() || b"/_-+".contains(b))
        {
            *position += 1;
        }
        let word = &input[start..*position];
        let name = String::from_utf8_lossy(word).into_owned();
        let abbreviation = abbreviation(word);
        if (abbreviation.is_none() || word == b"UTC") && Zone::named(word).is_some() {
            (
                ParsedZone::Identifier {
                    name,
                    abbreviation: abbreviation.map(|_| "UTC".into()),
                },
                true,
            )
        } else if let Some((seconds, daylight)) = abbreviation {
            (
                ParsedZone::Abbreviation {
                    seconds: seconds - i64::from(daylight) * 3600,
                    daylight,
                    name: name.to_ascii_uppercase(),
                },
                true,
            )
        } else {
            (ParsedZone::Invalid, false)
        }
    };
    while parentheses > 0 && input.get(*position) == Some(&b')') {
        parentheses -= 1;
        *position += 1;
    }
    result
}

fn abbreviation(word: &[u8]) -> Option<(i64, bool)> {
    for line in include_str!("../../data/abbreviations.tsv").lines() {
        let mut fields = line.split('\t');
        let name = fields.next()?;
        if !name.as_bytes().eq_ignore_ascii_case(word) {
            continue;
        }
        let daylight = fields.next()? == "1";
        return Some((fields.next()?.parse().ok()?, daylight));
    }
    None
}

fn number(bytes: &[u8]) -> i64 {
    bytes
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0, |value, byte| value * 10 + i64::from(byte - b'0'))
}

fn offset(bytes: &[u8]) -> Option<i64> {
    Some(match bytes.len() {
        1 | 2 => number(bytes) * 3600,
        3 | 4 if bytes.get(1) == Some(&b':') => number(bytes) * 3600 + number(&bytes[2..]) * 60,
        3 | 4 if bytes.get(2) == Some(&b':') => number(bytes) * 3600 + number(&bytes[3..]) * 60,
        3 | 4 => number(bytes) / 100 * 3600 + number(bytes) % 100 * 60,
        5 if bytes[2] == b':' => number(bytes) * 3600 + number(&bytes[3..]) * 60,
        6 => number(bytes) / 10000 * 3600 + number(bytes) / 100 % 100 * 60 + number(bytes) % 100,
        8 if bytes[2] == b':' && bytes[5] == b':' => {
            number(bytes) * 3600 + number(&bytes[3..]) * 60 + number(&bytes[6..])
        }
        _ => return None,
    })
}
