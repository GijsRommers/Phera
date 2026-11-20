//! Unicode mappings beyond ICU4X's normalizer API.
//!
//! ```php
//! Normalizer::getRawDecomposition('Ǆ', Normalizer::NFKD); // "DŽ"
//! Normalizer::normalize("Ａ\u{ad}", Normalizer::NFKC_CF); // "a"
//! ```

use std::sync::OnceLock;

use icu_normalizer::ComposingNormalizer;

struct Mapping {
    start: u32,
    end: u32,
    text: String,
}

fn parse(data: &str) -> Vec<Mapping> {
    let mut mappings: Vec<_> = data
        .lines()
        .filter_map(|row| {
            let (range, value) = row.split_once(';')?;
            let (start, end) = range.split_once("..").unwrap_or((range, range));
            Some(Mapping {
                start: u32::from_str_radix(start, 16).ok()?,
                end: u32::from_str_radix(end, 16).ok()?,
                text: value
                    .split_whitespace()
                    .map(|scalar| char::from_u32(u32::from_str_radix(scalar, 16).ok()?))
                    .collect::<Option<String>>()?,
            })
        })
        .collect();
    mappings.sort_unstable_by_key(|mapping| mapping.start);
    mappings
}

fn find(mappings: &[Mapping], character: char) -> Option<&str> {
    let scalar = u32::from(character);
    let index = mappings
        .partition_point(|mapping| mapping.start <= scalar)
        .checked_sub(1)?;
    let mapping = &mappings[index];
    (scalar <= mapping.end).then_some(&mapping.text)
}

pub fn compatibility(character: char) -> Option<&'static str> {
    static MAPPINGS: OnceLock<Vec<Mapping>> = OnceLock::new();
    find(
        MAPPINGS.get_or_init(|| parse(include_str!("../data/compatibility-decomposition.txt"))),
        character,
    )
}

pub fn casefold_mapping(character: char) -> Option<&'static str> {
    static MAPPINGS: OnceLock<Vec<Mapping>> = OnceLock::new();
    find(
        MAPPINGS.get_or_init(|| parse(include_str!("../data/nfkc-casefold.txt"))),
        character,
    )
}

pub fn casefold(text: &str) -> String {
    let mut mapped = String::with_capacity(text.len());
    for character in text.chars() {
        match casefold_mapping(character) {
            Some(mapping) => mapped.push_str(mapping),
            None => mapped.push(character),
        }
    }
    ComposingNormalizer::new_nfc()
        .normalize(&mapped)
        .into_owned()
}
