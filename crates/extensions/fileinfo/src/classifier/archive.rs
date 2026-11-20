//! File signatures and bounded container headers identify e.g. an uploaded `application/zip`.

use super::{Kind, le16, le32};

const ZIP_HEADER_SEARCH: usize = 256 * 1024;

pub(super) fn detect(bytes: &[u8], tail: &[u8]) -> Option<Kind> {
    if bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06") {
        return Some(zip(bytes));
    }
    if bytes.starts_with(b"\x1f\x8b\x08") {
        return Some(gzip(bytes, tail));
    }
    if let Some([b'B', b'Z', b'h', level @ b'1'..=b'9', ..]) = bytes.get(..) {
        let size = (level - b'0') as u16 * 100;
        return Some(Kind::new(
            "application/x-bzip2",
            "bz2",
            format!("bzip2 compressed data, block size = {size}k"),
        ));
    }
    if bytes.starts_with(b"\xfd7zXZ\x00") {
        let checksum = match bytes.get(7) {
            Some(1) => ", checksum CRC32",
            Some(4) => ", checksum CRC64",
            Some(10) => ", checksum SHA-256",
            _ => "",
        };
        return Some(Kind::new(
            "application/x-xz",
            "xz",
            format!("XZ compressed data{checksum}"),
        ));
    }
    if let [b'7', 0x7a, 0xbc, 0xaf, 0x27, 0x1c, major, minor, ..] = bytes {
        return Some(Kind::new(
            "application/x-7z-compressed",
            "7z/cb7",
            format!("7-zip archive data, version {major}.{minor}"),
        ));
    }
    if bytes.starts_with(b"Rar!\x1a\x07\x01\x00") {
        return Some(Kind::new(
            "application/vnd.rar",
            "rar",
            "RAR archive data, v5",
        ));
    }
    if bytes.starts_with(b"Rar!\x1a\x07\x00") {
        return Some(Kind::new(
            "application/x-rar",
            "rar",
            "RAR archive data, v2.0",
        ));
    }
    if bytes.get(257..262) == Some(b"ustar") {
        return Some(Kind::new(
            "application/x-tar",
            "tar/ustar",
            "POSIX tar archive",
        ));
    }
    None
}

fn zip(bytes: &[u8]) -> Kind {
    let mut entries = bytes;
    let mut office = false;
    for _ in 0..64 {
        let Some((name, data, next)) = zip_entry(entries) else {
            break;
        };
        if name == b"mimetype"
            && let Some(kind) = opendocument(data)
        {
            return kind;
        }
        office |= matches!(name, b"[Content_Types].xml" | b"_rels/.rels");
        if office && let Some(kind) = ooxml(name) {
            return kind;
        }
        entries = next;
    }
    let version = le16(bytes, 4).unwrap_or(20) & 0xff;
    let method = match le16(bytes, 8) {
        Some(0) | None => "store",
        Some(8) => "deflate",
        Some(12) => "bzip2",
        Some(14) => "lzma",
        _ => "unknown",
    };
    Kind::new(
        "application/zip",
        "???",
        format!(
            "Zip archive data, at least v{}.{} to extract, compression method={method}",
            version / 10,
            version % 10
        ),
    )
}

fn zip_entry(bytes: &[u8]) -> Option<(&[u8], &[u8], &[u8])> {
    if !bytes.starts_with(b"PK\x03\x04") {
        return None;
    }
    let name_end = 30 + usize::from(le16(bytes, 26)?);
    let data_start = name_end + usize::from(le16(bytes, 28)?);
    let name = bytes.get(30..name_end)?;
    if le16(bytes, 6)? & 8 != 0 {
        let (payload, next) = streamed_zip_entry(bytes.get(data_start..)?);
        return Some((name, payload, next));
    }
    let data_end = data_start.checked_add(le32(bytes, 18)? as usize)?;
    Some((
        name,
        bytes.get(data_start..data_end)?,
        bytes.get(data_end..)?,
    ))
}

fn streamed_zip_entry(bytes: &[u8]) -> (&[u8], &[u8]) {
    // ZIP writers that cannot seek put sizes after the data. Find the next
    // header within a bounded window without inflating the file.
    let scan = &bytes[..bytes.len().min(ZIP_HEADER_SEARCH)];
    let Some(offset) = scan
        .windows(4)
        .position(|magic| matches!(magic, b"PK\x03\x04" | b"PK\x01\x02"))
    else {
        return (&[], &[]);
    };
    let (framed, next) = bytes.split_at(offset);
    (zip_descriptor_payload(framed).unwrap_or_default(), next)
}

fn zip_descriptor_payload(framed: &[u8]) -> Option<&[u8]> {
    let descriptor = framed.len().checked_sub(12)?;
    let size = le32(framed, descriptor + 4)? as usize;
    if size == descriptor || framed.get(size..descriptor) == Some(b"PK\x07\x08") {
        framed.get(..size)
    } else {
        None
    }
}

fn ooxml(name: &[u8]) -> Option<Kind> {
    let (mime, ext, description) = if name.starts_with(b"word/") {
        (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "docx",
            "Microsoft Word 2007+",
        )
    } else if name.starts_with(b"xl/") {
        (
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "xlsx",
            "Microsoft Excel 2007+",
        )
    } else if name.starts_with(b"ppt/") {
        (
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "pptx",
            "Microsoft PowerPoint 2007+",
        )
    } else {
        return None;
    };
    Some(Kind::new(mime, ext, description))
}

fn opendocument(data: &[u8]) -> Option<Kind> {
    let (mime, ext, description) = match data {
        b"application/vnd.oasis.opendocument.text" => (
            "application/vnd.oasis.opendocument.text",
            "odt",
            "OpenDocument Text",
        ),
        b"application/vnd.oasis.opendocument.spreadsheet" => (
            "application/vnd.oasis.opendocument.spreadsheet",
            "ods",
            "OpenDocument Spreadsheet",
        ),
        b"application/vnd.oasis.opendocument.presentation" => (
            "application/vnd.oasis.opendocument.presentation",
            "odp",
            "OpenDocument Presentation",
        ),
        b"application/vnd.oasis.opendocument.graphics" => (
            "application/vnd.oasis.opendocument.graphics",
            "odg",
            "OpenDocument Drawing",
        ),
        _ => return None,
    };
    Some(Kind::new(mime, ext, description))
}

fn gzip(bytes: &[u8], tail: &[u8]) -> Kind {
    let mut description = String::from("gzip compressed data");
    if let Some(flags) = bytes.get(3).copied() {
        let mut offset = 10;
        if flags & 4 != 0 {
            offset += 2 + usize::from(le16(bytes, offset).unwrap_or_default());
        }
        if flags & 8 != 0
            && let Some(rest) = bytes.get(offset..)
        {
            let name = rest.split(|byte| *byte == 0).next().unwrap_or_default();
            description.push_str(&format!(", was \"{}\"", String::from_utf8_lossy(name)));
        }
    }
    match bytes.get(8) {
        Some(2) => description.push_str(", max compression"),
        Some(4) => description.push_str(", max speed"),
        _ => {}
    }
    let extension = if let Some(size) = le32(tail, 0) {
        description.push_str(&format!(", original size modulo 2^32 {size}"));
        "gz/tgz/tpz/ipk/vbox-extpack/svgz/blend/dia/gnucash/rdata/xoj"
    } else {
        "???"
    };
    Kind::new("application/gzip", extension, description)
}
