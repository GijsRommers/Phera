//! Bytes to code points and back, keeping invalid input visible:
//!
//! ```php
//! mb_scrub("a\xFFb");                          // "a?b": the bad byte became one character
//! mb_convert_encoding('é', 'ASCII', 'UTF-8');  // "?": not representable
//! mb_substitute_character('long');
//! mb_convert_encoding('é', 'ASCII', 'UTF-8');  // "U+E9"
//! ```
//!
//! Decoding turns every invalid sequence into [`BAD`], which counts as one
//! character, as in PHP. Encoding replaces what the target cannot represent
//! according to `mb_substitute_character()`.

pub mod cjk;
mod gb18030;
mod iso2022;
mod transfer;
mod unicode;

pub use transfer::hex_byte;

use crate::encoding::{Encoding, Family, read_u32};

/// The code point that stands for an invalid byte sequence.
pub const BAD: u32 = u32::MAX;

/// What replaces a character the target encoding cannot represent, as set
/// by `mb_substitute_character()`.
#[derive(Clone, Debug)]
pub enum Substitute {
    /// A replacement character, `?` by default.
    Character(char),
    /// `"none"`: leave the character out.
    None,
    /// `"long"`: `U+E9` for a character, the stored character for bad bytes.
    Long(char),
    /// `"entity"`: `&#xE9;` for a character, the stored character for bad bytes.
    Entity(char),
}

impl Default for Substitute {
    fn default() -> Self {
        Self::Character('?')
    }
}

pub fn decode(bytes: &[u8], encoding: Encoding) -> Vec<u32> {
    match encoding.family() {
        Family::Ascii => bytes
            .iter()
            .map(|&byte| if byte < 0x80 { u32::from(byte) } else { BAD })
            .collect(),
        Family::Bytes => bytes.iter().map(|&byte| u32::from(byte)).collect(),
        // Bytes without a character are `0xFFFFFFFF` in the table, which is `BAD`.
        Family::SingleByte(table) => bytes
            .iter()
            .map(|&byte| read_u32(&table[usize::from(byte) * 4..]))
            .collect(),
        Family::Ucs4(order) => unicode::decode_ucs4(bytes, order),
        Family::Ucs2(order) => unicode::decode_ucs2(bytes, order),
        Family::Utf16(order) => unicode::decode_utf16(bytes, order),
        Family::Utf8 => unicode::decode_utf8(bytes),
        Family::DoubleByte(tables) => cjk::decode(bytes, tables),
        Family::Iso2022Jp => iso2022::decode(bytes),
        Family::Gb18030 => gb18030::decode(bytes),
        Family::Transfer(transfer) => transfer::decode(bytes, transfer),
    }
}

pub fn encode(points: &[u32], encoding: Encoding, substitute: &Substitute) -> Vec<u8> {
    let family = encoding.family();
    match family {
        Family::Iso2022Jp => return iso2022::encode(points, substitute),
        Family::Transfer(transfer) => return transfer::encode(points, transfer),
        _ => {}
    }
    let mut output = Vec::with_capacity(points.len());
    if let Family::Utf8 = family {
        // Most strings are UTF-8: one loop without the dispatch of `encode_one`.
        for &point in points {
            match char::from_u32(point) {
                Some(character) if point < 0x80 => output.push(character as u8),
                Some(character) => {
                    output.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
                }
                None => encode_substitute(point, family, substitute, &mut output),
            }
        }
        return output;
    }
    for &point in points {
        if !encode_one(point, family, &mut output) {
            encode_substitute(point, family, substitute, &mut output);
        }
    }
    output
}

/// Appends `point` in an encoding that works one character at a time, or
/// returns false when the encoding cannot represent it.
fn encode_one(point: u32, family: Family, output: &mut Vec<u8>) -> bool {
    match family {
        Family::Ascii if point < 0x80 => output.push(point as u8),
        Family::Bytes if point < 0x100 => output.push(point as u8),
        // A table entry of `BAD` marks a byte without a character, so `BAD`
        // itself must not be looked up.
        Family::SingleByte(_) if point == BAD => return false,
        Family::SingleByte(table) => {
            let entries = table.as_chunks::<4>().0;
            let Some(byte) = entries.iter().position(|entry| read_u32(entry) == point) else {
                return false;
            };
            output.push(byte as u8);
        }
        // Lone surrogates are written as they are, as PHP does.
        Family::Ucs4(order) if point <= 0x10FFFF => {
            output.extend_from_slice(&unicode::bytes32(point, order));
        }
        Family::Ucs2(order) if point <= 0xFFFF => {
            output.extend_from_slice(&unicode::bytes16(point as u16, order));
        }
        Family::Utf16(order) if (0xD800..=0xDFFF).contains(&point) => {
            output.extend_from_slice(&unicode::bytes16(point as u16, order));
        }
        Family::Utf16(order) => {
            let Some(character) = char::from_u32(point) else {
                return false;
            };
            for unit in character.encode_utf16(&mut [0; 2]) {
                output.extend_from_slice(&unicode::bytes16(*unit, order));
            }
        }
        Family::Utf8 => {
            let Some(character) = char::from_u32(point) else {
                return false;
            };
            output.extend_from_slice(character.encode_utf8(&mut [0; 4]).as_bytes());
        }
        Family::DoubleByte(tables) => return cjk::encode(point, tables, output),
        Family::Gb18030 => return gb18030::encode(point, output),
        Family::Iso2022Jp | Family::Transfer(_) => {
            unreachable!("`encode` converts stateful encodings as a whole")
        }
        Family::Ascii | Family::Bytes | Family::Ucs4(_) | Family::Ucs2(_) => return false,
    }
    true
}

fn encode_substitute(point: u32, family: Family, substitute: &Substitute, output: &mut Vec<u8>) {
    match substitute {
        Substitute::None => {}
        Substitute::Character(character) => {
            encode_replacement(character.encode_utf8(&mut [0; 4]), family, output);
        }
        Substitute::Long(stored) | Substitute::Entity(stored) if point == BAD => {
            encode_one(u32::from(*stored), family, output);
        }
        Substitute::Long(_) => encode_replacement(&format!("U+{point:04X}"), family, output),
        Substitute::Entity(_) => encode_replacement(&format!("&#x{point:X};"), family, output),
    }
}

/// The replacement must fit the encoding too; where it does not, `?` does.
fn encode_replacement(text: &str, family: Family, output: &mut Vec<u8>) {
    for character in text.chars() {
        if !encode_one(u32::from(character), family, output) {
            encode_one(u32::from('?'), family, output);
        }
    }
}

/// The byte offset of every character, and the length at the end, judged
/// by lead bytes alone as PHP's `mb_str_split` and `mb_strcut` do: invalid
/// sequences are cut where they are, not decoded.
pub fn boundaries(bytes: &[u8], encoding: Encoding) -> Vec<usize> {
    let mut offsets = vec![0];
    let mut index = 0;
    while index < bytes.len() {
        index += character_length(bytes, index, encoding);
        offsets.push(index);
    }
    offsets
}

fn character_length(bytes: &[u8], index: usize, encoding: Encoding) -> usize {
    let lead = bytes[index];
    let next = bytes.get(index + 1).copied();
    if let Some(width) = encoding.fixed_width() {
        return width;
    }
    match encoding {
        Encoding::Utf16 | Encoding::Utf16Be | Encoding::Utf16Le => {
            let Some(next) = next else {
                return 2;
            };
            let unit = if encoding == Encoding::Utf16Le {
                u16::from_le_bytes([lead, next])
            } else {
                u16::from_be_bytes([lead, next])
            };
            if (0xD800..=0xDBFF).contains(&unit) {
                4
            } else {
                2
            }
        }
        Encoding::Utf8 => match lead {
            0xC2..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF4 => 4,
            _ => 1,
        },
        Encoding::EucJp => match lead {
            0x8F => 3,
            0x8E | 0xA1..=0xFE => 2,
            _ => 1,
        },
        Encoding::Sjis if matches!(lead, 0x81..=0x9F | 0xE0..=0xEF) => 2,
        Encoding::Cp932 | Encoding::SjisWin if matches!(lead, 0x81..=0x9F | 0xE0..=0xFF) => 2,
        Encoding::Cp936 | Encoding::Big5 if matches!(lead, 0x81..=0xFE) => 2,
        Encoding::EucKr if matches!(lead, 0xA1..=0xFE) => 2,
        // A digit after the lead byte starts a four-byte sequence.
        Encoding::Gb18030 if matches!(lead, 0x81..=0xFE) => {
            if next.is_some_and(|byte| byte.is_ascii_digit()) {
                4
            } else {
                2
            }
        }
        _ => 1,
    }
}
