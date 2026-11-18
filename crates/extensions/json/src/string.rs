//! A PHP string as a JSON string.
//!
//! ```php
//! json_encode("a/é\x01");                         // "a\/\u00e9\u0001"
//! json_encode("<a>", JSON_HEX_TAG);                // "\u003Ca\u003E"
//! json_encode("\xffa", JSON_INVALID_UTF8_IGNORE);  // "a"
//! ```

use crate::error::Error;
use crate::flags::{EncodeFlags, InvalidUtf8};
use std::fmt::Write;

/// Appends `bytes` to `out` as a quoted JSON string. On an error `out` may
/// hold part of the string; the caller truncates it.
pub(crate) fn encode_string(
    out: &mut String,
    bytes: &[u8],
    flags: EncodeFlags,
) -> Result<(), Error> {
    out.push('"');
    let mut remaining = bytes;
    while !remaining.is_empty() {
        let (sequence, rest) = remaining.split_at(sequence_length(remaining));
        if let Ok(text) = std::str::from_utf8(sequence) {
            for character in text.chars() {
                escape_char(character, flags, out);
            }
        } else {
            match flags.invalid_utf8 {
                InvalidUtf8::Ignore => {}
                InvalidUtf8::Substitute => escape_char(char::REPLACEMENT_CHARACTER, flags, out),
                InvalidUtf8::Fail => return Err(Error::Utf8),
            }
        }
        remaining = rest;
    }
    out.push('"');
    Ok(())
}

/// Length of the UTF-8 sequence that starts `bytes`, valid or not. As in
/// PHP, a malformed sequence ends before the first byte that cannot
/// continue it (an ASCII byte or a lead byte), and IGNORE or SUBSTITUTE
/// then applies to the sequence as a whole.
fn sequence_length(bytes: &[u8]) -> usize {
    let width = match bytes[0] {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 1,
    };
    let starts_a_character = |byte: &u8| *byte < 0x80 || (0xc2..=0xf4).contains(byte);
    bytes
        .iter()
        .take(width)
        .skip(1)
        .position(starts_a_character)
        .map_or(width.min(bytes.len()), |index| index + 1)
}

/// Appends one character, escaped as the flags ask. Escapes use lower-case
/// hex, except the `JSON_HEX_*` ones, which PHP writes in upper case.
fn escape_char(character: char, flags: EncodeFlags, out: &mut String) {
    match character {
        '"' if flags.hex_quot => out.push_str("\\u0022"),
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '/' if !flags.unescaped_slashes => out.push_str("\\/"),
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
        '\u{2028}' | '\u{2029}' if flags.unescaped_unicode && !flags.unescaped_line_terminators => {
            let _ = write!(out, "\\u{:04x}", character as u32);
        }
        character if character.is_ascii() || flags.unescaped_unicode => out.push(character),
        character => {
            let mut units = [0; 2];
            for unit in character.encode_utf16(&mut units) {
                let _ = write!(out, "\\u{unit:04x}");
            }
        }
    }
}
