//! Text probes share the encoding reported by `finfo_buffer($f, $text, FILEINFO_MIME_ENCODING)`.

use super::Kind;

#[derive(Clone, Copy)]
pub(super) enum Encoding {
    Binary,
    Ascii,
    Utf8,
    Utf8Bom,
    Utf16Le,
    Utf16Be,
    Latin1,
    Extended,
}

impl Encoding {
    pub(super) fn detect(bytes: &[u8]) -> Self {
        if bytes.len() < 2 {
            return Self::Binary;
        }
        if let Some(text) = bytes.strip_prefix(b"\xef\xbb\xbf")
            && valid_utf8(text)
        {
            return Self::Utf8Bom;
        }
        if bytes.iter().all(|byte| ascii(*byte)) {
            return Self::Ascii;
        }
        if valid_utf8(bytes) {
            return Self::Utf8;
        }
        for (bom, convert, encoding) in [
            (
                *b"\xff\xfe",
                u16::from_le_bytes as fn([u8; 2]) -> u16,
                Self::Utf16Le,
            ),
            (
                *b"\xfe\xff",
                u16::from_be_bytes as fn([u8; 2]) -> u16,
                Self::Utf16Be,
            ),
        ] {
            if let Some(text) = bytes.strip_prefix(&bom)
                && text.as_chunks::<2>().0.iter().all(|pair| {
                    let unit = convert([pair[0], pair[1]]);
                    unit >= 128 || ascii(unit as u8)
                })
            {
                return encoding;
            }
        }
        if bytes.iter().all(|byte| ascii(*byte) || *byte >= 0xa0) {
            return Self::Latin1;
        }
        if bytes.iter().all(|byte| ascii(*byte) || *byte >= 0x80) {
            return Self::Extended;
        }
        Self::Binary
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Ascii => "us-ascii",
            Self::Utf8 | Self::Utf8Bom => "utf-8",
            Self::Utf16Le => "utf-16le",
            Self::Utf16Be => "utf-16be",
            Self::Latin1 => "iso-8859-1",
            Self::Extended => "unknown-8bit",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Binary => "",
            Self::Ascii => "ASCII",
            Self::Utf8 => "Unicode text, UTF-8",
            Self::Utf8Bom => "Unicode text, UTF-8 (with BOM)",
            Self::Utf16Le => "Unicode text, UTF-16, little-endian",
            Self::Utf16Be => "Unicode text, UTF-16, big-endian",
            Self::Latin1 => "ISO-8859",
            Self::Extended => "Non-ISO extended-ASCII",
        }
    }

    fn content(self, bytes: &[u8]) -> String {
        match self {
            Self::Ascii | Self::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
            Self::Utf8Bom => String::from_utf8_lossy(&bytes[3..]).into_owned(),
            Self::Utf16Le | Self::Utf16Be => {
                let units = bytes[2..].as_chunks::<2>().0.iter().map(|pair| {
                    let pair = [pair[0], pair[1]];
                    if matches!(self, Self::Utf16Le) {
                        u16::from_le_bytes(pair)
                    } else {
                        u16::from_be_bytes(pair)
                    }
                });
                char::decode_utf16(units)
                    .map(|unit| unit.unwrap_or(char::REPLACEMENT_CHARACTER))
                    .collect()
            }
            _ => bytes.iter().copied().map(char::from).collect(),
        }
    }
}

fn ascii(byte: u8) -> bool {
    matches!(byte, 7..=13 | 27 | 32..=126 | 0x85)
}

fn valid_utf8(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok_and(|text| text.bytes().all(|byte| byte >= 128 || ascii(byte)))
}

pub(super) fn detect(bytes: &[u8], encoding: Encoding) -> Option<Kind> {
    if matches!(encoding, Encoding::Binary) {
        return None;
    }
    let text = encoding.content(bytes.get(..65_536).unwrap_or(bytes));
    let trimmed = text.trim_start();
    let prefix = trimmed.get(..trimmed.floor_char_boundary(trimmed.len().min(1024)))?;
    let lower = prefix.to_ascii_lowercase();
    let code = encoding.description();
    if matches!(trimmed.as_bytes().first(), Some(b'{' | b'['))
        && phera_ext_json::valid_json(trimmed.trim_end().as_bytes(), 128)
    {
        return Some(Kind::new("application/json", "???", "JSON text data"));
    }
    if csv(bytes) {
        return Some(Kind::new("text/csv", "???", format!("CSV {code} text")));
    }
    let (mime, extension, header) =
        if lower.starts_with("<svg") || (lower.starts_with("<?xml") && lower.contains("<svg")) {
            (
                "image/svg+xml",
                "svg",
                "SVG Scalable Vector Graphics image, ",
            )
        } else if lower.starts_with("<?php") || lower.starts_with("<?=") {
            ("text/x-php", "???", "PHP script, ")
        } else if lower.starts_with("<!doctype html") || lower.starts_with("<html") {
            ("text/html", "???", "HTML document, ")
        } else if lower.starts_with("<?xml") {
            ("text/xml", "???", "XML 1.0 document, ")
        } else {
            ("text/plain", "???", "")
        };
    Some(Kind::new(
        mime,
        extension,
        format!("{header}{code} text{}", line_style(&text)),
    ))
}

fn csv(bytes: &[u8]) -> bool {
    let mut quoted = false;
    let mut commas = 0;
    let mut first_line = 0;
    let mut lines = 0;
    for byte in bytes {
        match byte {
            b'"' => quoted = !quoted,
            b',' if !quoted => commas += 1,
            b'\n' if !quoted => {
                if lines == 0 {
                    first_line = commas;
                    if first_line < 2 {
                        return false;
                    }
                } else if commas != first_line {
                    return false;
                }
                lines += 1;
                commas = 0;
                if lines == 10 {
                    break;
                }
            }
            _ => {}
        }
    }
    lines >= 2
}

fn line_style(text: &str) -> String {
    let mut characters = text.chars().peekable();
    let (mut lf, mut crlf, mut cr, mut nel) = (false, false, false, false);
    while let Some(character) = characters.next() {
        match character {
            '\r' if characters.next_if_eq(&'\n').is_some() => crlf = true,
            '\r' if characters.peek().is_some() => cr = true,
            '\n' => lf = true,
            '\u{85}' => nel = true,
            _ => {}
        }
    }
    if !(lf || crlf || cr || nel) {
        return ", with no line terminators".into();
    }
    if lf && !(crlf || cr || nel) {
        return String::new();
    }
    let styles = [(crlf, "CRLF"), (cr, "CR"), (lf, "LF"), (nel, "NEL")]
        .into_iter()
        .filter_map(|(present, name)| present.then_some(name))
        .collect::<Vec<_>>();
    format!(", with {} line terminators", styles.join(", "))
}
