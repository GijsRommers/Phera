//! The Unicode encodings, where an invalid unit is one bad character:
//!
//! ```php
//! mb_strlen("\xFF\xFEa\x00", 'UTF-16');  // 1: a little-endian byte order mark, then "a"
//! mb_strlen("\x00a\x00", 'UTF-16BE');    // 2: "a" and a left-over byte
//! mb_strlen("\xD8\x00", 'UTF-16BE');     // 1: a lone surrogate
//! ```

use super::BAD;
use crate::encoding::ByteOrder;

pub fn decode_utf8(bytes: &[u8]) -> Vec<u32> {
    let mut points = Vec::with_capacity(bytes.len());
    for chunk in bytes.utf8_chunks() {
        points.extend(chunk.valid().chars().map(u32::from));
        if !chunk.invalid().is_empty() {
            points.push(BAD);
        }
    }
    points
}

pub fn decode_ucs4(bytes: &[u8], order: ByteOrder) -> Vec<u32> {
    let (bytes, little) = without_mark(bytes, order, &[0, 0, 0xFE, 0xFF], &[0xFF, 0xFE, 0, 0]);
    let (units, rest) = bytes.as_chunks::<4>();
    let mut points: Vec<u32> = units
        .iter()
        .map(|&chunk| {
            let point = if little {
                u32::from_le_bytes(chunk)
            } else {
                u32::from_be_bytes(chunk)
            };
            checked(point)
        })
        .collect();
    if !rest.is_empty() {
        points.push(BAD);
    }
    points
}

pub fn decode_ucs2(bytes: &[u8], order: ByteOrder) -> Vec<u32> {
    let (units, left_over) = units16(bytes, order);
    let mut points: Vec<u32> = units.map(|unit| checked(u32::from(unit))).collect();
    if left_over {
        points.push(BAD);
    }
    points
}

pub fn decode_utf16(bytes: &[u8], order: ByteOrder) -> Vec<u32> {
    let (units, left_over) = units16(bytes, order);
    let mut points: Vec<u32> = char::decode_utf16(units)
        .map(|unit| unit.map_or(BAD, u32::from))
        .collect();
    if left_over {
        points.push(BAD);
    }
    points
}

/// The point itself if it is a Unicode scalar value, `BAD` for a surrogate
/// or anything above U+10FFFF.
fn checked(point: u32) -> u32 {
    if char::from_u32(point).is_some() {
        point
    } else {
        BAD
    }
}

/// The 16-bit units of the string, and whether an odd byte is left over.
fn units16(bytes: &[u8], order: ByteOrder) -> (impl Iterator<Item = u16> + '_, bool) {
    let (bytes, little) = without_mark(bytes, order, &[0xFE, 0xFF], &[0xFF, 0xFE]);
    let (units, rest) = bytes.as_chunks::<2>();
    let units = units.iter().map(move |&unit| {
        if little {
            u16::from_le_bytes(unit)
        } else {
            u16::from_be_bytes(unit)
        }
    });
    (units, !rest.is_empty())
}

/// For an encoding whose byte order a mark decides, removes the mark and
/// says whether the rest is little-endian.
fn without_mark<'a>(
    bytes: &'a [u8],
    order: ByteOrder,
    big_mark: &[u8],
    little_mark: &[u8],
) -> (&'a [u8], bool) {
    match order {
        ByteOrder::Big => (bytes, false),
        ByteOrder::Little => (bytes, true),
        ByteOrder::Marked => {
            if let Some(rest) = bytes.strip_prefix(big_mark) {
                (rest, false)
            } else if let Some(rest) = bytes.strip_prefix(little_mark) {
                (rest, true)
            } else {
                (bytes, false)
            }
        }
    }
}

/// Encoding never writes a byte order mark: a marked encoding is written
/// big-endian.
pub fn bytes32(point: u32, order: ByteOrder) -> [u8; 4] {
    if order == ByteOrder::Little {
        point.to_le_bytes()
    } else {
        point.to_be_bytes()
    }
}

pub fn bytes16(unit: u16, order: ByteOrder) -> [u8; 2] {
    if order == ByteOrder::Little {
        unit.to_le_bytes()
    } else {
        unit.to_be_bytes()
    }
}
