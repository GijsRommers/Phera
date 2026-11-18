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
    if let Some(number) = tail.strip_prefix('#') {
        let (digits, radix, prefix) = if let Some(hex) = number.strip_prefix(['x', 'X']) {
            (hex, 16, 2)
        } else {
            (number, 10, 1)
        };
        let end = digits
            .bytes()
            .take_while(|byte| byte.is_ascii_digit() || radix == 16 && byte.is_ascii_hexdigit())
            .count();
        if digits.as_bytes().get(end) != Some(&b';') {
            return None;
        }
        let value = u32::from_str_radix(&digits[..end], radix).ok()?;
        return (value <= 0x10ffff).then_some(prefix + end);
    }

    let end = tail.bytes().take_while(u8::is_ascii_alphanumeric).count();
    if tail.as_bytes().get(end) != Some(&b';') {
        return None;
    }
    let name = &tail.as_bytes()[..end];
    ENTITIES
        .iter()
        .any(|(_, encoded)| encoded.as_bytes().get(1..encoded.len() - 1) == Some(name))
        .then_some(end)
}
