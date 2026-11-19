//! PHP exposes and lets callers replace number prefixes, suffixes and symbols.
//!
//! ```php
//! (new NumberFormatter('en_US', NumberFormatter::CURRENCY))->getTextAttribute(0); // "$"
//! ```

use fixed_decimal::{Decimal, Sign};
use icu_experimental::dimension::currency::{
    CurrencyType, formatter::CurrencyFormatter, options::CurrencyUsage,
};
use std::rc::Rc;

use super::{CurrencyRender, Formatter, Style, attributes};
use crate::number_data;

pub(super) fn currency_formatter(state: &mut Formatter, code: &str) -> Option<Rc<CurrencyRender>> {
    if let Some(cached) = state.currency_cache.get(code) {
        return Some(Rc::clone(cached));
    }
    let currency = CurrencyType::try_from_str(&code.to_ascii_lowercase()).ok()?;
    let preferences = state.locale.clone().into();
    let usage = if state.style == Style::CurrencyAccounting {
        CurrencyUsage::Accounting
    } else {
        CurrencyUsage::Standard
    };
    let formatter = match state.style {
        Style::CurrencyIso => CurrencyFormatter::try_new_code(preferences, currency, usage.into()),
        Style::CurrencyPlural => CurrencyFormatter::try_new_name(preferences, currency),
        _ => CurrencyFormatter::try_new_symbol(preferences, currency, usage.into()),
    }
    .ok()?;
    let cached = Rc::new(CurrencyRender {
        formatter,
        fraction: number_data::fraction(code, state.style == Style::CurrencyCash),
    });
    if state.currency_cache.len() >= 32 {
        state.currency_cache.clear();
    }
    state
        .currency_cache
        .insert(code.to_owned(), Rc::clone(&cached));
    Some(cached)
}

pub fn currency_symbol(state: &Formatter) -> String {
    let Some(currency) = CurrencyType::try_from_str(&state.currency.to_ascii_lowercase()).ok()
    else {
        return "¤".to_owned();
    };
    let Some(formatter) = CurrencyFormatter::try_new_symbol(
        state.locale.clone().into(),
        currency,
        Default::default(),
    )
    .ok() else {
        return state.currency.clone();
    };
    let output = formatter
        .format_fixed_decimal(&Decimal::from(1))
        .to_string();
    let (prefix, _, suffix) = split_number(&output);
    let cleaned = format!("{prefix}{suffix}");
    cleaned
        .trim_matches(|ch: char| ch.is_whitespace() || is_bidi_mark(ch))
        .to_owned()
}

fn is_bidi_mark(ch: char) -> bool {
    matches!(ch, '\u{061c}' | '\u{200e}' | '\u{200f}')
}

pub fn split_number(text: &str) -> (&str, &str, &str) {
    let first = text.find(|ch| digit(ch).is_some()).unwrap_or(text.len());
    let end = text
        .rfind(|ch| digit(ch).is_some())
        .and_then(|index| {
            text[index..]
                .chars()
                .next()
                .map(|last| index + last.len_utf8())
        })
        .unwrap_or(first);
    (&text[..first], &text[first..end], &text[end..])
}

pub fn digit(character: char) -> Option<u32> {
    use icu_properties::{CodePointMapData, props::GeneralCategory};
    let categories = CodePointMapData::<GeneralCategory>::new();
    if character.is_ascii_digit() {
        return character.to_digit(10);
    }
    if categories.get(character) != GeneralCategory::DecimalNumber {
        return None;
    }
    let scalar = u32::from(character);
    let mut start = scalar;
    while start > 0 && categories.get32(start - 1) == GeneralCategory::DecimalNumber {
        start -= 1;
    }
    Some((scalar - start) % 10)
}

pub fn pattern_affixes(state: &Formatter, currency: &str, negative: bool) -> (String, String) {
    let (positive, negative_pattern) = state
        .pattern
        .split_once(';')
        .map_or((state.pattern.as_str(), None), |(pos, neg)| {
            (pos, Some(neg))
        });
    let pattern = if negative {
        negative_pattern.unwrap_or(positive)
    } else {
        positive
    };
    let digits = attributes::numeric_pattern(pattern);
    let index = pattern.find(digits).unwrap_or_default();
    let prefix = &pattern[..index];
    let suffix = pattern[index + digits.len()..]
        .strip_prefix("E0")
        .unwrap_or(&pattern[index + digits.len()..]);
    let sign = if negative && negative_pattern.is_none() {
        attributes::symbol(state, 6).unwrap_or_default()
    } else {
        String::new()
    };
    let translate = |part: &str| {
        let data = state.data;
        let output = if part.contains('¤') {
            let money = if state.currency == currency {
                currency_symbol(state)
            } else {
                currency.to_owned()
            };
            part.replace("¤¤", currency).replace('¤', &money)
        } else {
            part.to_owned()
        };
        output
            .replace(
                '%',
                state
                    .symbols
                    .get(&3)
                    .map_or(data.symbols[3], String::as_str),
            )
            .replace(
                '‰',
                state
                    .symbols
                    .get(&12)
                    .map_or(data.symbols[8], String::as_str),
            )
            .replace("'", "")
    };
    (sign + &translate(prefix), translate(suffix))
}

pub fn affixes(
    state: &Formatter,
    value: &Decimal,
    prefix: &str,
    number: &str,
    suffix: &str,
) -> String {
    let first = if value.sign == Sign::Negative { 2 } else { 0 };
    let prefix = state
        .text_attributes
        .get(&first)
        .map_or(prefix, String::as_str);
    let suffix = state
        .text_attributes
        .get(&(first + 1))
        .map_or(suffix, String::as_str);
    let width = state.attributes.width.map_or(0, |width| width as usize);
    let pad = state
        .text_attributes
        .get(&4)
        .map_or(" ", String::as_str)
        .chars()
        .next()
        .unwrap_or(' ');
    let amount = width
        .saturating_sub(prefix.chars().count() + number.chars().count() + suffix.chars().count());
    let padding: String = std::iter::repeat_n(pad, amount).collect();
    match state.attributes.pad_position {
        1 => format!("{prefix}{padding}{number}{suffix}"),
        2 => format!("{prefix}{number}{padding}{suffix}"),
        3 => format!("{prefix}{number}{suffix}{padding}"),
        _ => format!("{padding}{prefix}{number}{suffix}"),
    }
}
