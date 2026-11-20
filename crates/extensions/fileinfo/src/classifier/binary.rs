//! Recognize PDF, RTF, Office compound files, fonts and executables by their headers.

use super::{Kind, be16, be32, le16, le32};

pub(super) fn detect(bytes: &[u8]) -> Option<Kind> {
    if let [b'%', b'P', b'D', b'F', b'-', major, b'.', minor, ..] = bytes {
        return Some(
            Kind::new(
                "application/pdf",
                "pdf",
                format!(
                    "PDF document, version {}.{}",
                    char::from(*major),
                    char::from(*minor)
                ),
            )
            .apple("CAROPDF "),
        );
    }
    if let Some(data) = bytes.strip_prefix(b"{\\rtf") {
        let version: String = data
            .iter()
            .copied()
            .take_while(u8::is_ascii_digit)
            .map(char::from)
            .collect();
        let encoding = if data.get(version.len()..)?.starts_with(b"\\ansi") {
            ", ANSI"
        } else {
            ""
        };
        return Some(Kind::new(
            "text/rtf",
            "rtf",
            format!("Rich Text Format data, version {version}{encoding}"),
        ));
    }
    if bytes.starts_with(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1") {
        return Some(compound(bytes));
    }
    if bytes.starts_with(b"\x7fELF") {
        return Some(elf(bytes));
    }
    if matches!(
        bytes.get(..4),
        Some(b"\xfe\xed\xfa\xce" | b"\xce\xfa\xed\xfe" | b"\xfe\xed\xfa\xcf" | b"\xcf\xfa\xed\xfe")
    ) {
        return Some(macho(bytes));
    }
    if bytes.starts_with(b"MZ") {
        return Some(pe(bytes));
    }
    fonts(bytes)
}

fn compound(bytes: &[u8]) -> Kind {
    let find_directory = || {
        let sector_size = 1usize.checked_shl(u32::from(le16(bytes, 30)?))?;
        if !matches!(sector_size, 512 | 4096) {
            return None;
        }
        let sector = (le32(bytes, 48)? as usize).checked_add(1)?;
        let start = sector.checked_mul(sector_size)?;
        let end = start.checked_add(sector_size)?;
        bytes.get(start..end)
    };
    let Some(directory) = find_directory() else {
        return Kind::new(
            "application/x-ole-storage",
            "???",
            "Composite Document File V2 Document",
        );
    };
    for entry in directory.as_chunks::<128>().0.iter() {
        if entry[66] != 2 {
            continue;
        }
        let length = usize::from(le16(entry, 64).unwrap_or(0)).saturating_sub(2);
        let Some(name) = entry[..64].get(..length) else {
            continue;
        };
        let units: Vec<u16> = name
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let name = String::from_utf16_lossy(&units);
        let (mime, ext, product) = match name.as_str() {
            "Workbook" | "Book" => ("application/vnd.ms-excel", "xls", "Excel"),
            "WordDocument" => ("application/msword", "doc", "Word"),
            "PowerPoint Document" => ("application/vnd.ms-powerpoint", "ppt", "PowerPoint"),
            _ => continue,
        };
        return Kind::new(mime, ext, format!("CDFV2 Microsoft {product}"));
    }
    Kind::new(
        "application/x-ole-storage",
        "???",
        "Composite Document File V2 Document",
    )
}

fn elf(bytes: &[u8]) -> Kind {
    let big = bytes.get(5) == Some(&2);
    let endian = if big { "MSB" } else { "LSB" };
    let read = if big { be16 } else { le16 };
    let size = if bytes.get(4) == Some(&2) { 64 } else { 32 };
    let (mime, kind) = match read(bytes, 16) {
        Some(1) => ("application/x-object", "relocatable"),
        Some(2) => ("application/x-executable", "executable"),
        Some(3) => {
            if elf_has_interpreter(bytes, size == 64, big) {
                ("application/x-pie-executable", "pie executable")
            } else {
                ("application/x-sharedlib", "shared object")
            }
        }
        Some(4) => ("application/x-coredump", "core file"),
        _ => ("application/octet-stream", "unknown type"),
    };
    let architecture = match read(bytes, 18) {
        Some(3) => "Intel 80386",
        Some(8) => "MIPS",
        Some(20) => "PowerPC",
        Some(21) => "64-bit PowerPC",
        Some(40) => "ARM",
        Some(62) => "x86-64",
        Some(183) => "ARM aarch64",
        Some(243) => "RISC-V",
        _ => "unknown architecture",
    };
    let abi = match bytes.get(7) {
        Some(3) => "GNU/Linux",
        Some(9) => "FreeBSD",
        _ => "SYSV",
    };
    Kind::new(
        mime,
        "???",
        format!("ELF {size}-bit {endian} {kind}, {architecture}, version 1 ({abi})"),
    )
}

fn elf_has_interpreter(bytes: &[u8], wide: bool, big: bool) -> bool {
    let u16_at = if big { be16 } else { le16 };
    let u32_at = if big { be32 } else { le32 };
    let table = if wide {
        bytes.get(32..40).and_then(|offset| {
            let offset = offset.try_into().ok()?;
            let offset = if big {
                u64::from_be_bytes(offset)
            } else {
                u64::from_le_bytes(offset)
            };
            usize::try_from(offset).ok()
        })
    } else {
        u32_at(bytes, 28).map(|offset| offset as usize)
    };
    let Some(table) = table.and_then(|offset| bytes.get(offset..)) else {
        return false;
    };
    let fields = if wide { 54 } else { 42 };
    let size = usize::from(u16_at(bytes, fields).unwrap_or(0));
    if size < 4 {
        return false;
    }
    let count = usize::from(u16_at(bytes, fields + 2).unwrap_or(0)).min(2048);
    table
        .chunks_exact(size)
        .take(count)
        .any(|entry| u32_at(entry, 0) == Some(3))
}

fn macho(bytes: &[u8]) -> Kind {
    let big = bytes.first() == Some(&0xfe);
    let read = if big { be32 } else { le32 };
    let size = if matches!(bytes.first(), Some(0xcf)) || bytes.get(3) == Some(&0xcf) {
        64
    } else {
        32
    };
    let architecture = match read(bytes, 4) {
        Some(7) => "i386",
        Some(0x0100_0007) => "x86_64",
        Some(12) => "arm",
        Some(0x0100_000c) => "arm64",
        _ => "unknown architecture",
    };
    let kind = match read(bytes, 12) {
        Some(1) => "object",
        Some(2) => "executable",
        Some(6) => "dynamically linked shared library",
        _ => "binary",
    };
    Kind::new(
        "application/x-mach-binary",
        "16/18/",
        format!("Mach-O {size}-bit {architecture} {kind}"),
    )
}

fn pe(bytes: &[u8]) -> Kind {
    let header = le32(bytes, 60).and_then(|offset| bytes.get(offset as usize..));
    if let Some(pe) = header.and_then(|header| header.strip_prefix(b"PE\0\0")) {
        let architecture = match le16(pe, 0) {
            Some(0x14c) => "Intel 80386",
            Some(0x8664) => "x86-64",
            Some(0xaa64) => "Aarch64",
            _ => "unknown architecture",
        };
        let signature = le16(pe, 20).unwrap_or_default();
        let kind = match signature {
            0x10b => "PE32 executable".into(),
            0x20b => "PE32+ executable".into(),
            _ => format!("PE with unknown signature {signature:x}"),
        };
        return Kind::new(
            "application/vnd.microsoft.portable-executable",
            "???",
            format!("{kind}, {architecture}"),
        );
    }
    Kind::new("application/x-dosexec", "exe", "MS-DOS executable")
}

fn fonts(bytes: &[u8]) -> Option<Kind> {
    if bytes.starts_with(b"OTTO") {
        return Some(Kind::new(
            "application/vnd.ms-opentype",
            "???",
            "OpenType font data",
        ));
    }
    if bytes.starts_with(b"\0\x01\0\0")
        && let Some(tag) = bytes.get(12..16)
        && tag.iter().all(|byte| matches!(byte, 32..=126))
    {
        let tables = be16(bytes, 4)?;
        let tag = std::str::from_utf8(tag).ok()?;
        return Some(Kind::new(
            "font/sfnt",
            "ttf/tte",
            format!("TrueType Font data, {tables} tables, 1st \"{tag}\""),
        ));
    }
    let (mime, ext, version, position) = match bytes.get(..4) {
        Some(b"wOFF") => ("font/woff", "???", "", 20),
        Some(b"wOF2") => ("font/woff2", "woff2", " (Version 2)", 24),
        _ => return None,
    };
    let flavor = if bytes.get(4..8) == Some(b"OTTO") {
        "CFF"
    } else {
        "TrueType"
    };
    let length = be32(bytes, 8)?;
    let major = be16(bytes, position).unwrap_or_default();
    let minor = be16(bytes, position + 2).unwrap_or_default();
    Some(Kind::new(
        mime,
        ext,
        format!(
            "Web Open Font Format{version}, {flavor}, length {length}, version {major}.{minor}"
        ),
    ))
}
