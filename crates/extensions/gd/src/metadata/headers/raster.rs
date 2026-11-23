//! `getimagesize` reads the BMP, TIFF, ICO, WebP and ISO image container headers.

use super::{Size, be16, be32, le16, le32};

pub(super) fn bmp(data: &[u8]) -> Option<Size> {
    match le32(data, 14)? {
        12 => Some(Size::new(
            le16(data, 18)?.into(),
            le16(data, 20)?.into(),
            le16(data, 24)?,
            0,
        )),
        40..=124 => Some(Size::new(
            (le32(data, 18)? as i32).unsigned_abs(),
            (le32(data, 22)? as i32).unsigned_abs(),
            le16(data, 28)?,
            0,
        )),
        _ => None,
    }
}

pub(super) fn ico(data: &[u8]) -> Option<Size> {
    let count = usize::from(le16(data, 4)?);
    let directory = data.get(6..)?;
    let selected = directory
        .as_chunks::<16>()
        .0
        .iter()
        .take(count)
        .max_by_key(|entry| le16(entry.as_slice(), 6).unwrap_or(0))?;
    let dimension = |byte: u8| if byte == 0 { 256 } else { u32::from(byte) };
    Some(Size::new(
        dimension(selected[0]),
        dimension(selected[1]),
        le16(selected, 6)?,
        0,
    ))
}

pub(super) fn webp(data: &[u8]) -> Option<Size> {
    let chunk = data.get(20..)?;
    let (width, height) = match (data.get(12..16)?, chunk) {
        (b"VP8X", [_, _, _, _, a, b, c, d, e, f, ..]) => (
            u32::from_le_bytes([*a, *b, *c, 0]) + 1,
            u32::from_le_bytes([*d, *e, *f, 0]) + 1,
        ),
        (b"VP8L", [0x2f, a, b, c, d, ..]) => {
            let size = u32::from_le_bytes([*a, *b, *c, *d]);
            ((size & 0x3fff) + 1, ((size >> 14) & 0x3fff) + 1)
        }
        (b"VP8 ", [_, _, _, 0x9d, 1, 0x2a, ..]) => (
            u32::from(le16(chunk, 6)? & 0x3fff),
            u32::from(le16(chunk, 8)? & 0x3fff),
        ),
        _ => return None,
    };
    Some(Size::new(width, height, 8, 0))
}

pub(super) fn tiff(data: &[u8]) -> Option<Size> {
    let big = data.starts_with(b"MM");
    let read16 = if big { be16 } else { le16 };
    let read32 = if big { be32 } else { le32 };
    let directory = data.get(read32(data, 4)? as usize..)?;
    let count = usize::from(read16(directory, 0)?);
    let mut width = None;
    let mut height = None;
    for item in directory.get(2..)?.as_chunks::<12>().0.iter().take(count) {
        let slot = match read16(item, 0)? {
            256 => &mut width,
            257 => &mut height,
            _ => continue,
        };
        *slot = match read16(item, 2)? {
            3 => read16(item, 8).map(u32::from),
            4 => read32(item, 8),
            _ => None,
        };
    }
    Some(Size::new(width?, height?, 0, 0))
}

pub(super) fn ispe(data: &[u8]) -> Option<Size> {
    data.windows(20).find_map(|entry| {
        if entry.get(4..8) != Some(b"ispe") || be32(entry, 0)? < 20 {
            return None;
        }
        Some(Size::new(be32(entry, 12)?, be32(entry, 16)?, 0, 0))
    })
}
