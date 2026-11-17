//! PHP searches UTF-8 bytes after simple folding: `mb_stripos('ßa', 'a') === 1`.
use crate::{
    codec::{BAD, decode},
    encoding::Encoding,
    unicode::{self, Case},
};
pub fn searchable(bytes: &[u8], encoding: Encoding, fold: bool) -> Vec<u8> {
    if encoding == Encoding::Utf8 && !fold {
        return bytes.to_vec();
    }
    let mut points = decode(bytes, encoding);
    if fold {
        points = unicode::convert(&points, Case::FoldSimple);
    }
    let mut out = Vec::new();
    for cp in points {
        if cp == BAD {
            out.push(0xff);
        } else if let Some(c) = char::from_u32(cp) {
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
        }
    }
    out
}
fn count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|b| **b & 0xc0 != 0x80).count()
}
fn pointer(bytes: &[u8], offset: i64) -> Option<usize> {
    if offset < 0 {
        let mut remaining = offset.unsigned_abs();
        for i in (0..bytes.len()).rev() {
            if bytes[i] & 0xc0 != 0x80 {
                remaining -= 1;
                if remaining == 0 {
                    return Some(i);
                }
            }
        }
        None
    } else {
        let mut i = 0;
        for _ in 0..offset {
            if i >= bytes.len() {
                return None;
            }
            i += match bytes[i] {
                0xc2..=0xdf => 2,
                0xe0..=0xef => 3,
                0xf0..=0xf4 => 4,
                _ => 1,
            };
        }
        Some(i.min(bytes.len()))
    }
}
pub fn find(
    haystack: &[u8],
    needle: &[u8],
    offset: i64,
    reverse: bool,
) -> Result<Option<usize>, ()> {
    let position = pointer(haystack, offset).ok_or(())?;
    let (from, end) = if reverse && offset < 0 {
        let extra = pointer(&haystack[position..], count(needle) as i64)
            .unwrap_or(haystack.len() - position);
        (0, position + extra)
    } else {
        (position, haystack.len())
    };
    if needle.is_empty() {
        return Ok(Some(count(&haystack[..if reverse { end } else { from }])));
    }
    if needle.len() > end - from {
        return Ok(None);
    }
    let mut matches = (from..=end - needle.len()).filter(|i| haystack[*i..].starts_with(needle));
    let found = if reverse {
        matches.next_back()
    } else {
        matches.next()
    };
    Ok(found.map(|i| count(&haystack[..i])))
}
