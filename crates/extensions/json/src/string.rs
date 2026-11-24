//! A PHP string as a JSON string.
//!
//! ```php
//! json_encode("a/é\x01");                         // "a\/\u00e9\u0001"
//! json_encode("<a>", JSON_HEX_TAG);                // "\u003Ca\u003E"
//! json_encode("\xffa", JSON_INVALID_UTF8_IGNORE);  // "a"
//! ```

use crate::error::Error;
use crate::flags::{EncodeFlags, InvalidUtf8};

const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";

/// Appends `bytes` to `out` as a quoted JSON string. On an error `out` may
/// hold part of the string; the caller truncates it.
pub(crate) fn encode_string(
    out: &mut Vec<u8>,
    bytes: &[u8],
    flags: EncodeFlags,
) -> Result<(), Error> {
    out.reserve(bytes.len().saturating_add(2));
    out.push(b'"');
    let mut remaining = bytes;
    while !remaining.is_empty() {
        let plain = remaining
            .iter()
            .take_while(|&&byte| plain_ascii(byte, flags))
            .count();
        let (run, rest) = remaining.split_at(plain);
        // Every byte in `run` is proven ASCII (`0x20..=0x7f`), so it is valid
        // UTF-8 and needs no second validation pass.
        out.extend_from_slice(run);
        if rest.is_empty() {
            break;
        }
        let first = rest[0];
        if first < 0x80 {
            escape_char(first as char, flags, out);
            remaining = &rest[1..];
            continue;
        }
        let (sequence, rest) = rest.split_at(sequence_length(rest));
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
    out.push(b'"');
    Ok(())
}

/// ASCII bytes that need no escaping under `flags`.
#[inline]
fn plain_ascii(byte: u8, flags: EncodeFlags) -> bool {
    if !(b' '..=0x7f).contains(&byte) {
        return false;
    }
    match byte {
        b'"' | b'\\' => false,
        b'/' => flags.unescaped_slashes,
        b'<' | b'>' => !flags.hex_tag,
        b'&' => !flags.hex_amp,
        b'\'' => !flags.hex_apos,
        _ => true,
    }
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

#[inline]
fn push_unicode_escape(out: &mut Vec<u8>, unit: u16) {
    out.extend_from_slice(&[
        b'\\',
        b'u',
        HEX_LOWER[((unit >> 12) & 0xf) as usize],
        HEX_LOWER[((unit >> 8) & 0xf) as usize],
        HEX_LOWER[((unit >> 4) & 0xf) as usize],
        HEX_LOWER[(unit & 0xf) as usize],
    ]);
}

/// Appends one character, escaped as the flags ask. Escapes use lower-case
/// hex, except the `JSON_HEX_*` ones, which PHP writes in upper case.
fn escape_char(character: char, flags: EncodeFlags, out: &mut Vec<u8>) {
    match character {
        '"' if flags.hex_quot => out.extend_from_slice(b"\\u0022"),
        '"' => out.extend_from_slice(b"\\\""),
        '\\' => out.extend_from_slice(b"\\\\"),
        '/' if !flags.unescaped_slashes => out.extend_from_slice(b"\\/"),
        '\u{8}' => out.extend_from_slice(b"\\b"),
        '\u{c}' => out.extend_from_slice(b"\\f"),
        '\n' => out.extend_from_slice(b"\\n"),
        '\r' => out.extend_from_slice(b"\\r"),
        '\t' => out.extend_from_slice(b"\\t"),
        '<' if flags.hex_tag => out.extend_from_slice(b"\\u003C"),
        '>' if flags.hex_tag => out.extend_from_slice(b"\\u003E"),
        '&' if flags.hex_amp => out.extend_from_slice(b"\\u0026"),
        '\'' if flags.hex_apos => out.extend_from_slice(b"\\u0027"),
        character if character < ' ' => push_unicode_escape(out, character as u16),
        '\u{2028}' | '\u{2029}' if flags.unescaped_unicode && !flags.unescaped_line_terminators => {
            push_unicode_escape(out, character as u16);
        }
        character if character.is_ascii() => out.push(character as u8),
        character if flags.unescaped_unicode => {
            let mut buf = [0u8; 4];
            out.extend_from_slice(character.encode_utf8(&mut buf).as_bytes());
        }
        character => {
            let mut units = [0u16; 2];
            for unit in character.encode_utf16(&mut units) {
                push_unicode_escape(out, *unit);
            }
        }
    }
}
