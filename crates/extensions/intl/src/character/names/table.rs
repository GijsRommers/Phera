//! Unicode 18 explicit, range-generated, and Hangul character names.
//!
//! ```php
//! IntlChar::charName('가'); // "HANGUL SYLLABLE GA"
//! IntlChar::charName('一'); // "CJK UNIFIED IDEOGRAPH-4E00"
//! ```

use std::sync::OnceLock;

const HANGUL_START: u32 = 0xac00;
const HANGUL_END: u32 = 0xd7a3;
const LEAD: [&str; 19] = [
    "G", "GG", "N", "D", "DD", "R", "M", "B", "BB", "S", "SS", "", "J", "JJ", "C", "K", "T", "P",
    "H",
];
const VOWEL: [&str; 21] = [
    "A", "AE", "YA", "YAE", "EO", "E", "YEO", "YE", "O", "WA", "WAE", "OE", "YO", "U", "WEO", "WE",
    "WI", "YU", "EU", "YI", "I",
];
const TRAIL: [&str; 28] = [
    "", "G", "GG", "GS", "N", "NJ", "NH", "D", "L", "LG", "LM", "LB", "LS", "LT", "LP", "LH", "M",
    "B", "BS", "S", "SS", "NG", "J", "C", "K", "T", "P", "H",
];

#[derive(Default)]
struct Names {
    names: Vec<(u32, &'static str)>,
    aliases: Vec<(u32, &'static str)>,
    ranges: Vec<(u32, u32, &'static str)>,
}

fn names() -> &'static Names {
    static NAMES: OnceLock<Names> = OnceLock::new();
    NAMES.get_or_init(|| {
        let mut data = Names::default();
        for row in include_str!("../../../data/unicode-character-names.tsv").lines() {
            let Some((code, name)) = row.split_once('\t') else {
                continue;
            };
            if let Some(code) = code.strip_prefix('!') {
                if let Ok(code) = u32::from_str_radix(code, 16) {
                    data.aliases.push((code, name));
                }
            } else if let Some(prefix) = name.strip_suffix('*') {
                let (start, end) = code.split_once("..").unwrap_or((code, code));
                if let (Ok(start), Ok(end)) =
                    (u32::from_str_radix(start, 16), u32::from_str_radix(end, 16))
                {
                    data.ranges.push((start, end, prefix));
                }
            } else if let Ok(code) = u32::from_str_radix(code, 16) {
                data.names.push((code, name));
            }
        }
        data.aliases.sort_unstable_by_key(|&(code, _)| code);
        data
    })
}

pub(super) fn explicit(code: u32, alias: bool) -> Option<&'static str> {
    let rows = if alias {
        &names().aliases
    } else {
        &names().names
    };
    let index = rows.binary_search_by_key(&code, |&(code, _)| code).ok()?;
    rows.get(index).map(|&(_, name)| name)
}

pub(super) fn unicode(code: u32) -> Option<String> {
    if (HANGUL_START..=HANGUL_END).contains(&code) {
        let syllable = (code - HANGUL_START) as usize;
        let lead = LEAD[syllable / 588];
        let vowel = VOWEL[syllable % 588 / 28];
        let trail = TRAIL[syllable % 28];
        return Some(format!("HANGUL SYLLABLE {lead}{vowel}{trail}"));
    }
    if let Some(name) = explicit(code, false) {
        return Some(name.to_owned());
    }
    names().ranges.iter().find_map(|&(start, end, prefix)| {
        (start..=end)
            .contains(&code)
            .then(|| format!("{prefix}{code:04X}"))
    })
}

fn loose(name: &str) -> impl Iterator<Item = u8> {
    let bytes = name.as_bytes();
    bytes
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, byte)| {
            let medial_hyphen = byte == b'-'
                && index > 0
                && bytes[index - 1].is_ascii_alphanumeric()
                && bytes.get(index + 1).is_some_and(u8::is_ascii_alphanumeric);
            (!byte.is_ascii_whitespace() && byte != b'_' && !medial_hyphen)
                .then_some(byte.to_ascii_uppercase())
        })
}

pub(super) fn reverse(name: &str, alias: bool) -> Option<u32> {
    let key: Vec<_> = loose(name).collect();
    let data = names();
    if !alias {
        if let Some(syllable) = key.strip_prefix(b"HANGULSYLLABLE") {
            return hangul(syllable);
        }
        for &(start, end, prefix) in &data.ranges {
            let prefix: Vec<_> = loose(prefix.trim_end_matches('-')).collect();
            if let Some(hex) = key.strip_prefix(prefix.as_slice()) {
                let code = u32::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
                if (start..=end).contains(&code) {
                    return Some(code);
                }
            }
        }
    }
    let rows = if alias { &data.aliases } else { &data.names };
    rows.iter()
        .find_map(|&(code, candidate)| loose(candidate).eq(key.iter().copied()).then_some(code))
}

fn hangul(name: &[u8]) -> Option<u32> {
    for (lead, prefix) in LEAD.iter().enumerate() {
        let Some(rest) = name.strip_prefix(prefix.as_bytes()) else {
            continue;
        };
        for (vowel, prefix) in VOWEL.iter().enumerate() {
            if let Some(trail) = rest.strip_prefix(prefix.as_bytes())
                && let Some(trail) = TRAIL.iter().position(|suffix| suffix.as_bytes() == trail)
            {
                return Some(HANGUL_START + (lead * 588 + vowel * 28 + trail) as u32);
            }
        }
    }
    None
}
