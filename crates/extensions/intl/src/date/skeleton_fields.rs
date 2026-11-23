//! Compare skeleton fields and adjust a localized pattern to the requested widths.
//!
//! ```php
//! IntlDatePatternGenerator::create('en_US')->getBestPattern('yyyyMMdd'); // "MM/dd/yyyy"
//! ```

use super::Style;

#[derive(Clone, Copy)]
pub(super) struct Field {
    pub symbol: char,
    pub width: usize,
    key: u8,
}

impl Field {
    pub fn is_time(self) -> bool {
        self.key >= 13
    }

    pub fn text(self) -> String {
        self.symbol.to_string().repeat(self.width)
    }
}

fn key(symbol: char) -> Option<u8> {
    Some(match symbol {
        'G' => 0,
        'y' | 'Y' | 'u' | 'U' | 'r' => 1,
        'Q' | 'q' => 2,
        'M' | 'L' => 3,
        'l' => 4,
        'w' => 5,
        'W' => 6,
        'E' | 'e' | 'c' => 7,
        'd' => 8,
        'D' => 9,
        'F' => 10,
        'g' => 11,
        'a' | 'b' | 'B' => 13,
        'h' | 'H' | 'K' | 'k' | 'j' | 'J' | 'C' => 14,
        'm' => 15,
        's' => 16,
        'S' => 17,
        'A' => 18,
        'z' | 'Z' | 'O' | 'v' | 'V' | 'X' | 'x' => 19,
        _ => return None,
    })
}

fn spans(text: &str) -> impl Iterator<Item = (&str, Option<Field>)> {
    let mut rest = text;
    let mut quoted = false;
    std::iter::from_fn(move || {
        let symbol = rest.chars().next()?;
        let size = symbol.len_utf8();
        let (width, key) = if symbol == '\'' {
            if rest.starts_with("''") {
                (2, None)
            } else {
                quoted = !quoted;
                (1, None)
            }
        } else if let Some(key) = key(symbol).filter(|_| !quoted) {
            (
                rest.bytes()
                    .take_while(|byte| *byte == symbol as u8)
                    .count(),
                Some(key),
            )
        } else {
            (1, None)
        };
        let (span, tail) = rest.split_at(size * width);
        rest = tail;
        Some((span, key.map(|key| Field { symbol, width, key })))
    })
}

pub(super) fn read(text: &str) -> Vec<Field> {
    let mut fields: Vec<_> = spans(text).filter_map(|(_, field)| field).collect();
    fields.sort_by_key(|field| field.key);
    fields.dedup_by_key(|field| field.key);
    fields
}

pub(super) fn mask(fields: &[Field]) -> u32 {
    fields.iter().fold(0, |mask, field| mask | 1 << field.key)
}

pub(super) fn distance(requested: &[Field], candidate: &[Field]) -> usize {
    requested
        .iter()
        .zip(candidate)
        .map(|(requested, candidate)| {
            let width = if requested.key == 1 {
                usize::from((requested.width == 2) != (candidate.width == 2)) * 4
            } else {
                requested.width.abs_diff(candidate.width)
            };
            let symbol = usize::from(requested.symbol != candidate.symbol);
            let numeric =
                matches!(requested.key, 2 | 3) && (requested.width <= 2) != (candidate.width <= 2);
            width + symbol * 8 + usize::from(numeric) * 16
        })
        .sum()
}

pub(super) fn date_style(fields: &[Field]) -> Style {
    let month = fields.iter().find(|field| field.key == 3);
    match month.map(|field| field.width) {
        Some(4..)
            if fields
                .iter()
                .any(|field| field.key == 7 && field.width >= 4) =>
        {
            Style::Full
        }
        Some(4..) => Style::Long,
        Some(3) => Style::Medium,
        _ => Style::Short,
    }
}

pub(super) fn adjust(pattern: &str, requested: &[Field]) -> String {
    let mut output = String::with_capacity(pattern.len());
    for (span, field) in spans(pattern) {
        let Some((actual, desired)) = field.and_then(|actual| {
            requested
                .iter()
                .find(|desired| desired.key == actual.key)
                .map(|desired| (actual, desired))
        }) else {
            output.push_str(span);
            continue;
        };
        let mut symbol = actual.symbol;
        let width = match actual.key {
            1 => {
                symbol = desired.symbol;
                desired.width
            }
            2 | 3 if (actual.width <= 2) == (desired.width <= 2) => actual.width.max(desired.width),
            7 if matches!(symbol, 'E' | 'c') => desired.width.max(3),
            8 if desired.width == 2 => 2,
            _ => actual.width,
        };
        output.extend(std::iter::repeat_n(symbol, width));
    }
    output
}

pub(super) fn without_period(pattern: &str) -> String {
    let mut output = String::with_capacity(pattern.len());
    let mut after_period = false;
    for (span, field) in spans(pattern) {
        if field.is_some_and(|field| field.key == 13) {
            output.truncate(output.trim_end().len());
            after_period = true;
        } else if !(after_period && span.trim().is_empty()) {
            if let Some(Field {
                symbol: 'h',
                width: 1,
                ..
            }) = field
            {
                output.push_str("hh");
            } else {
                output.push_str(span);
            }
            after_period = false;
        }
    }
    output
}
