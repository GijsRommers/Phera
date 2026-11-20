//! Image headers provide e.g. `finfo_buffer($f, $png) === 'PNG image data, …'`.

use super::{Kind, be16, be32, le16, le32};

pub(super) fn detect(bytes: &[u8]) -> Option<Kind> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.get(12..16) == Some(b"IHDR") {
        return Some(png(bytes));
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        let version = std::str::from_utf8(bytes.get(3..6)?).ok()?;
        let (width, height) = (le16(bytes, 6)?, le16(bytes, 8)?);
        return Some(
            Kind::new(
                "image/gif",
                "gif",
                format!("GIF image data, version {version}, {width} x {height}"),
            )
            .apple("8BIMGIFf"),
        );
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        return Some(jpeg(bytes));
    }
    if bytes.starts_with(b"BM")
        && matches!(le32(bytes, 14), Some(12 | 40 | 52 | 56 | 64 | 108 | 124))
    {
        return Some(bmp(bytes));
    }
    if bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*") {
        return Some(tiff(bytes));
    }
    if bytes.starts_with(b"\0\0\x01\0") && le16(bytes, 4)? != 0 && bytes.get(6..22).is_some() {
        return Some(ico(bytes));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return Some(webp(bytes));
    }
    None
}

fn png(bytes: &[u8]) -> Kind {
    let description = match bytes.get(24..29) {
        Some([depth, color, _, _, interlace]) => {
            let width = be32(bytes, 16).unwrap_or_default();
            let height = be32(bytes, 20).unwrap_or_default();
            let color = match color {
                0 => "grayscale",
                2 => "color RGB",
                3 => "colormap",
                4 => "gray+alpha",
                6 => "color RGBA",
                _ => "unknown color",
            };
            let interlace = if *interlace == 0 {
                "non-interlaced"
            } else {
                "interlaced"
            };
            format!("PNG image data, {width} x {height}, {depth}-bit/{color}, {interlace}")
        }
        _ => "PNG image data".into(),
    };
    Kind::new("image/png", "png", description)
}

fn jpeg(bytes: &[u8]) -> Kind {
    let mut description = String::from("JPEG image data");
    let mut remaining = bytes.get(2..).unwrap_or_default();
    while let Some(marker) = remaining.strip_prefix(b"\xff") {
        if marker.first() == Some(&0xff) {
            remaining = marker;
            continue;
        }
        let Some(kind) = marker.first().copied() else {
            break;
        };
        if matches!(kind, 0xda | 0xd9) {
            break;
        }
        let Some(length) = be16(marker, 1).filter(|length| *length >= 2) else {
            break;
        };
        let end = usize::from(length) + 1;
        let Some(data) = marker.get(3..end) else {
            break;
        };
        jpeg_segment(&mut description, kind, data, length);
        remaining = &marker[end..];
    }
    Kind::new("image/jpeg", "jpeg/jpg/jpe/jfif", description).apple("8BIMJPEG")
}

fn jpeg_segment(out: &mut String, kind: u8, data: &[u8], length: u16) {
    use std::fmt::Write;

    match (kind, data) {
        (
            0xe0,
            [
                b'J',
                b'F',
                b'I',
                b'F',
                0,
                major,
                minor,
                units,
                x1,
                x2,
                y1,
                y2,
                ..,
            ],
        ) => {
            let units = match units {
                0 => "aspect ratio",
                1 => "resolution (DPI)",
                2 => "resolution (DPCM)",
                _ => "resolution",
            };
            let x = u16::from_be_bytes([*x1, *x2]);
            let y = u16::from_be_bytes([*y1, *y2]);
            let _ = write!(
                out,
                ", JFIF standard {major}.{minor:02}, {units}, density {x}x{y},"
            );
            let _ = write!(out, " segment length {length}");
        }
        (0xe1, [b'E', b'x', b'i', b'f', 0, 0, ..]) => out.push_str(", Exif standard"),
        (0xfe, _) => {
            let comment = data
                .split(|byte| *byte == 0)
                .next()
                .unwrap_or_default()
                .trim_ascii_end();
            let _ = write!(out, ", comment: \"{}\"", String::from_utf8_lossy(comment));
        }
        (0xc0..=0xcf, [precision, h1, h2, w1, w2, count, ..])
            if !matches!(kind, 0xc4 | 0xc8 | 0xcc) =>
        {
            let mode = match kind {
                0xc0 => "baseline",
                0xc1 => "extended sequential",
                0xc2 => "progressive",
                0xc3 => "lossless",
                _ => "unknown",
            };
            let width = u16::from_be_bytes([*w1, *w2]);
            let height = u16::from_be_bytes([*h1, *h2]);
            let _ = write!(
                out,
                ", {mode}, precision {precision}, {width}x{height}, components {count}"
            );
        }
        _ => {}
    }
}

fn bmp(bytes: &[u8]) -> Kind {
    let header = le32(bytes, 14).unwrap_or_default();
    let width = le32(bytes, 18).unwrap_or_default() as i32;
    let height = le32(bytes, 22).unwrap_or_default() as i32;
    let depth = le16(bytes, 28).unwrap_or_default();
    let size = le32(bytes, 2).unwrap_or_default();
    let offset = le32(bytes, 10).unwrap_or_default();
    let mut description = match header {
        12 => format!(
            "PC bitmap, OS/2 1.x format, {} x {} x {}",
            le16(bytes, 18).unwrap_or_default(),
            le16(bytes, 20).unwrap_or_default(),
            le16(bytes, 24).unwrap_or_default(),
        ),
        _ => format!("PC bitmap, Windows 3.x format, {width} x {height} x {depth}"),
    };
    if let Some(image_size) = le32(bytes, 34).filter(|size| *size > 0 && header >= 40) {
        description.push_str(&format!(", image size {image_size}"));
    }
    description.push_str(&format!(", cbSize {size}, bits offset {offset}"));
    Kind::new("image/bmp", "bmp/ico", description)
}

fn ico(bytes: &[u8]) -> Kind {
    let count = le16(bytes, 4).unwrap_or_default();
    let dimensions = |value| if value == 0 { 256 } else { u16::from(value) };
    let width = dimensions(bytes[6]);
    let height = dimensions(bytes[7]);
    let depth = le16(bytes, 12).unwrap_or_default();
    let plural = if count == 1 { "" } else { "s" };
    Kind::new(
        "image/vnd.microsoft.icon",
        "ico",
        format!(
            "MS Windows icon resource - {count} icon{plural}, {width}x{height}, {depth} bits/pixel"
        ),
    )
}

fn tiff(bytes: &[u8]) -> Kind {
    use std::fmt::Write;

    let big = bytes.starts_with(b"MM");
    let u16_at = if big { be16 } else { le16 };
    let u32_at = if big { be32 } else { le32 };
    let endian = if big { "big" } else { "little" };
    let mut description = format!("TIFF image data, {endian}-endian");
    if let Some(ifd) = u32_at(bytes, 4).and_then(|offset| bytes.get(offset as usize..))
        && let Some(count) = u16_at(ifd, 0)
    {
        let _ = write!(description, ", direntries={count}");
        for entry in ifd[2..].chunks_exact(12).take(usize::from(count)) {
            let name = match u16_at(entry, 0) {
                Some(256) => "width",
                Some(257) => "height",
                Some(258) => "bps",
                Some(259) => "compression",
                _ => continue,
            };
            let value = match u16_at(entry, 2) {
                Some(3) => u16_at(entry, 8).map(u32::from),
                Some(4) => u32_at(entry, 8),
                _ => None,
            };
            if let Some(value) = value {
                let _ = write!(description, ", {name}={value}");
            }
        }
    }
    Kind::new("image/tiff", "tif/tiff", description).apple("8BIMTIFF")
}

fn webp(bytes: &[u8]) -> Kind {
    let mut description = String::from("RIFF (little-endian) data, Web/P image");
    match (bytes.get(12..16), bytes.get(20..)) {
        (Some(b"VP8X"), Some([flags, _, _, _, x1, x2, x3, y1, y2, y3, ..])) => {
            if flags & 0x10 != 0 {
                description.push_str(", with alpha");
            }
            let width = u32::from_le_bytes([*x1, *x2, *x3, 0]);
            let height = u32::from_le_bytes([*y1, *y2, *y3, 0]);
            description.push_str(&format!(", {width}+1x{height}+1"));
        }
        (Some(b"VP8 "), Some([_, _, _, 0x9d, 0x01, 0x2a, w1, w2, h1, h2, ..])) => {
            let width = u16::from_le_bytes([*w1, *w2]) & 0x3fff;
            let height = u16::from_le_bytes([*h1, *h2]) & 0x3fff;
            description.push_str(&format!(", VP8 encoding, {width}x{height}, Scaling: [none]x[none], YUV color, decoders should clamp"));
        }
        _ => {}
    }
    Kind::new("image/webp", "webp", description)
}
