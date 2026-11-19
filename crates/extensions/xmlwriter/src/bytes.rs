//! XMLWriter preserves PHP string bytes in UTF-8 mode and transcodes on output.
//!
//! ```php
//! $writer->startDocument(encoding: 'ISO-8859-1');
//! $writer->writeElement('example', 'é 🦀');
//! // Output contains an ISO-8859-1 byte for é and a character reference for 🦀.
//! ```

use encoding_rs::Encoding as LegacyEncoding;

#[derive(Default)]
pub enum Encoding {
    #[default]
    Utf8,
    Utf16Le {
        bom: bool,
    },
    Utf16Be,
    Latin1,
    Ascii,
    Legacy(&'static LegacyEncoding),
}

impl Encoding {
    pub fn from_label(label: Option<&[u8]>) -> Result<Self, &'static str> {
        let Some(label) = label else {
            return Ok(Self::Utf8);
        };
        match label.to_ascii_uppercase().as_slice() {
            b"UTF-8" | b"UTF8" => Ok(Self::Utf8),
            b"UTF-16" => Ok(Self::Utf16Le { bom: true }),
            b"UTF-16LE" => Ok(Self::Utf16Le { bom: false }),
            b"UTF-16BE" => Ok(Self::Utf16Be),
            b"ISO-8859-1" | b"ISO8859-1" | b"LATIN1" => Ok(Self::Latin1),
            b"US-ASCII" | b"ASCII" => Ok(Self::Ascii),
            _ => LegacyEncoding::for_label_no_replacement(label)
                .map(Self::Legacy)
                .ok_or("xmlTextWriterStartDocument : unsupported encoding"),
        }
    }

    pub fn convert(&mut self, output: Vec<u8>) -> Vec<u8> {
        if matches!(self, Self::Utf8) {
            return output;
        }
        let input = String::from_utf8_lossy(&output);
        let mut output = Vec::with_capacity(input.len());
        match self {
            Self::Utf8 => return input.into_owned().into_bytes(),
            Self::Utf16Le { bom } => {
                if std::mem::take(bom) {
                    output.extend([0xff, 0xfe]);
                }
                for unit in input.encode_utf16() {
                    output.extend(unit.to_le_bytes());
                }
            }
            Self::Utf16Be => {
                for unit in input.encode_utf16() {
                    output.extend(unit.to_be_bytes());
                }
            }
            Self::Latin1 | Self::Ascii => {
                let max = if matches!(self, Self::Ascii) {
                    127
                } else {
                    255
                };
                for character in input.chars() {
                    if character as u32 <= max {
                        output.push(character as u8);
                    } else {
                        let reference = format!("&#{};", character as u32);
                        output.extend_from_slice(reference.as_bytes());
                    }
                }
            }
            Self::Legacy(encoding) => return encoding.encode(&input).0.into_owned(),
        }
        output
    }
}

pub fn nul_prefix(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|byte| *byte == 0) {
        Some(end) => &bytes[..end],
        None => bytes,
    }
}

pub fn qualified(prefix: Option<&[u8]>, name: &[u8]) -> Vec<u8> {
    let Some(prefix) = prefix else {
        return name.to_vec();
    };
    let mut qualified = Vec::with_capacity(prefix.len() + name.len() + 1);
    qualified.extend_from_slice(prefix);
    qualified.push(b':');
    qualified.extend_from_slice(name);
    qualified
}

pub fn escaped(output: &mut Vec<u8>, input: &[u8], attribute: bool) {
    let mut start = 0;
    for (index, byte) in input.iter().copied().enumerate() {
        let escaped: &[u8] = match byte {
            b'&' => b"&amp;",
            b'<' => b"&lt;",
            b'>' => b"&gt;",
            b'"' => b"&quot;",
            b'\r' => b"&#13;",
            b'\n' if attribute => b"&#10;",
            b'\t' if attribute => b"&#9;",
            _ => continue,
        };
        output.extend_from_slice(&input[start..index]);
        output.extend_from_slice(escaped);
        start = index + 1;
    }
    output.extend_from_slice(&input[start..]);
}
