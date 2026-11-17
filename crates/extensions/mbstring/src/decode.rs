//! Decode to code points, retaining invalid units: `mb_scrub("\xff") === "?"`.
use crate::encoding::{Encoding, read_u32};
pub const BAD: u32 = u32::MAX;

pub fn decode(bytes: &[u8], encoding: Encoding) -> Vec<u32> {
    if encoding.0 >= 44 {
        return crate::pseudo::decode(bytes, encoding);
    }
    if let Some(points) = crate::cjk::decode(bytes, encoding) {
        return points;
    }
    if let Some(table) = encoding.single() {
        return bytes
            .iter()
            .map(|b| read_u32(&table[usize::from(*b) * 4..]))
            .collect();
    }
    match encoding.0 {
        0 | 15 => bytes
            .iter()
            .map(|b| if *b < 128 { u32::from(*b) } else { BAD })
            .collect(),
        1 => bytes.iter().map(|b| u32::from(*b)).collect(),
        2..=13 => units(bytes, encoding),
        14 => utf8(bytes),
        20 => crate::iso2022::decode(bytes),
        21 => gb18030(bytes, encoding),
        _ => web(bytes, encoding),
    }
}
fn utf8(mut bytes: &[u8]) -> Vec<u32> {
    let mut out = Vec::with_capacity(bytes.len());
    loop {
        match std::str::from_utf8(bytes) {
            Ok(valid) => {
                out.extend(valid.chars().map(u32::from));
                break;
            }
            Err(error) => {
                let (valid, rest) = bytes.split_at(error.valid_up_to());
                if let Ok(valid) = std::str::from_utf8(valid) {
                    out.extend(valid.chars().map(u32::from));
                }
                out.push(BAD);
                match error.error_len() {
                    Some(length) => bytes = &rest[length..],
                    None => break,
                }
            }
        }
    }
    out
}
fn units(mut bytes: &[u8], encoding: Encoding) -> Vec<u32> {
    let wide = matches!(encoding.0, 2..=4 | 8..=10);
    let automatic = matches!(encoding.0, 2 | 5 | 8 | 11);
    let mut little = matches!(encoding.0, 4 | 7 | 10 | 13);
    if automatic {
        let (be, le): (&[u8], &[u8]) = if wide {
            (&[0, 0, 254, 255], &[255, 254, 0, 0])
        } else {
            (&[254, 255], &[255, 254])
        };
        if bytes.starts_with(be) {
            bytes = &bytes[be.len()..];
        } else if bytes.starts_with(le) {
            bytes = &bytes[le.len()..];
            little = true;
        }
    }
    let size = if wide { 4 } else { 2 };
    let mut out = Vec::with_capacity(bytes.len() / size);
    let mut chunks = bytes.chunks_exact(size);
    while let Some(chunk) = chunks.next() {
        let point = if wide {
            let b = [chunk[0], chunk[1], chunk[2], chunk[3]];
            if little {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            }
        } else {
            u32::from(if little {
                u16::from_le_bytes([chunk[0], chunk[1]])
            } else {
                u16::from_be_bytes([chunk[0], chunk[1]])
            })
        };
        if matches!(encoding.0, 11..=13) && (0xd800..=0xdbff).contains(&point) {
            let rest = chunks.clone().next().unwrap_or_default();
            if rest.len() >= 2 {
                let next = u32::from(if little {
                    u16::from_le_bytes([rest[0], rest[1]])
                } else {
                    u16::from_be_bytes([rest[0], rest[1]])
                });
                if (0xdc00..=0xdfff).contains(&next) {
                    chunks.next();
                    out.push(0x10000 + ((point - 0xd800) << 10) + next - 0xdc00);
                    continue;
                }
            }
            out.push(BAD);
        } else if char::from_u32(point).is_some() {
            out.push(point);
        } else {
            out.push(BAD);
        }
    }
    if !chunks.remainder().is_empty() {
        out.push(BAD);
    }
    out
}
fn web(bytes: &[u8], encoding: Encoding) -> Vec<u32> {
    let Some(web) = encoding.web() else {
        return Vec::new();
    };
    let mut decoder = web.new_decoder_without_bom_handling();
    let mut text = String::with_capacity(bytes.len() * 4 + 16);
    let mut out = Vec::new();
    let mut offset = 0;
    loop {
        let (result, read) =
            decoder.decode_to_string_without_replacement(&bytes[offset..], &mut text, true);
        offset += read;
        out.extend(text.chars().map(u32::from));
        text.clear();
        match result {
            encoding_rs::DecoderResult::InputEmpty => break,
            encoding_rs::DecoderResult::Malformed(_, _) => out.push(BAD),
            encoding_rs::DecoderResult::OutputFull => text.reserve(bytes.len() * 4 + 16),
        }
    }
    out
}

fn gb18030(bytes: &[u8], encoding: Encoding) -> Vec<u32> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] < 0x80 {
            out.push(u32::from(bytes[i]));
            i += 1;
        } else if bytes[i] == 0x80 || bytes[i] == 0xff {
            out.push(BAD);
            i += 1;
        } else {
            let length = if bytes.get(i + 1).is_some_and(u8::is_ascii_digit) {
                4
            } else {
                2
            };
            let end = (i + length).min(bytes.len());
            if let Some(cp) = crate::gb18030::decode(&bytes[i..end]) {
                out.push(cp);
            } else {
                out.extend(web(&bytes[i..end], encoding));
            }
            i = end;
        }
    }
    out
}
