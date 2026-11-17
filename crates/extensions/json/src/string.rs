//! JSON string escaping with PHP option precedence.
use crate::{error::Error, flags::Flags};
use std::fmt::Write;
/// PHP groups malformed sequences before applying IGNORE or SUBSTITUTE.
pub(crate) fn encode_string(bytes: &[u8], flags: Flags) -> Result<String, Error> {
    let mut out = String::with_capacity(bytes.len() + 2);
    out.push('"');

    let mut remaining = bytes;
    while let Some(&first) = remaining.first() {
        let width = match first {
            0..=127 => 1,
            194..=223 => 2,
            224..=239 => 3,
            240..=244 => 4,
            _ => 1,
        };
        let consumed = remaining
            .iter()
            .take(width)
            .enumerate()
            .skip(1)
            .find(|(_, byte)| **byte < 128 || (194..=244).contains(*byte))
            .map_or(width.min(remaining.len()), |(index, _)| index);
        let (sequence, rest) = remaining.split_at(consumed);
        if let Ok(text) = std::str::from_utf8(sequence) {
            for character in text.chars() {
                escape_char(character, flags, &mut out);
            }
        } else if flags.ignore {
        } else if flags.substitute {
            escape_char(char::REPLACEMENT_CHARACTER, flags, &mut out);
        } else {
            return Err(Error::Utf8);
        }
        remaining = rest;
    }

    out.push('"');
    Ok(out)
}

/// Emit ASCII escapes or UTF-16 pairs with PHP's exact hex casing.
fn escape_char(character: char, flags: Flags, out: &mut String) {
    match character {
        '"' if flags.hex_quot => out.push_str("\\u0022"),
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '/' if !flags.slashes => out.push_str("\\/"),
        '\u{8}' => out.push_str("\\b"),
        '\u{c}' => out.push_str("\\f"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '<' if flags.hex_tag => out.push_str("\\u003C"),
        '>' if flags.hex_tag => out.push_str("\\u003E"),
        '&' if flags.hex_amp => out.push_str("\\u0026"),
        '\'' if flags.hex_apos => out.push_str("\\u0027"),
        character if character < ' ' => {
            let _ = write!(out, "\\u{:04x}", character as u32);
        }
        '\u{2028}' | '\u{2029}' if flags.unicode && !flags.lines => {
            let _ = write!(out, "\\u{:04x}", character as u32);
        }
        character if character.is_ascii() || flags.unicode => out.push(character),
        character => {
            let mut units = [0; 2];
            for unit in character.encode_utf16(&mut units) {
                let _ = write!(out, "\\u{unit:04x}");
            }
        }
    }
}
