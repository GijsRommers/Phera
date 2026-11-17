//! The full special-character filter encodes HTML 4 entities once.
//!
//! ```php
//! filter_var("é &copy; &", FILTER_SANITIZE_FULL_SPECIAL_CHARS);
//! // "&eacute; &copy; &amp;"
//! ```

use crate::constants::NO_ENCODE_QUOTES;
use crate::html_table::ENTITIES;
use crate::options::Flags;

pub fn full(input: &[u8], flags: Flags) -> String {
    let Ok(text) = std::str::from_utf8(input) else {
        // PHP calls its entity encoder without ENT_IGNORE or ENT_SUBSTITUTE.
        return String::new();
    };
    let mut result = String::with_capacity(text.len());
    let mut chars = text.char_indices();
    while let Some((index, ch)) = chars.next() {
        if ch == '&'
            && let Some(length) = entity_length(&text[index + 1..])
        {
            result.push_str(&text[index..index + length + 2]);
            for _ in 0..=length {
                chars.next();
            }
        } else if flags.has(NO_ENCODE_QUOTES) && matches!(ch, '\'' | '"') {
            result.push(ch);
        } else if let Ok(index) = ENTITIES.binary_search_by_key(&ch, |&(key, _)| key) {
            result.push_str(ENTITIES[index].1);
        } else {
            result.push(ch);
        }
    }
    result
}

fn entity_length(tail: &str) -> Option<usize> {
    let end = tail
        .as_bytes()
        .iter()
        .take(32)
        .position(|&byte| byte == b';')?;
    let name = &tail[..end];
    let valid = if let Some(number) = name.strip_prefix('#') {
        let (digits, radix) = if let Some(hex) = number.strip_prefix(['x', 'X']) {
            (hex, 16)
        } else {
            (number, 10)
        };
        !digits.starts_with(['+', '-'])
            && u32::from_str_radix(digits, radix).is_ok_and(|value| value <= 0x10ffff)
    } else {
        ENTITIES.iter().any(|(_, encoded)| {
            encoded.as_bytes().get(1..encoded.len() - 1) == Some(name.as_bytes())
        })
    };
    valid.then_some(end)
}
