//! The transfer formats mbstring treats as encodings, deprecated since PHP 8.2:
//!
//! ```php
//! mb_convert_encoding('abc', 'BASE64', '8bit');        // "YWJj"
//! mb_convert_encoding('é', 'HTML-ENTITIES', 'UTF-8');  // "&eacute;"
//! mb_strlen('YWJj', 'BASE64');                          // 3: the decoded bytes
//! ```
//!
//! Decoding gives bytes as code points below 256 (HTML entities give real
//! code points); encoding takes the low byte of each point.

use std::sync::OnceLock;

use base64::Engine;

use super::BAD;
use crate::encoding::Transfer;

/// PHP's MIME line length for BASE64.
const BASE64_LINE: usize = 76;
/// Bytes per line of uuencoded output.
const UUENCODE_LINE: usize = 45;
/// The column after which Quoted-Printable breaks a line with `=\r\n`.
const QUOTED_LINE: usize = 72;

pub fn decode(bytes: &[u8], transfer: Transfer) -> Vec<u32> {
    let bytes = match transfer {
        Transfer::Base64 => return base64_decode(bytes),
        Transfer::HtmlEntities => return html_decode(bytes),
        Transfer::QuotedPrintable => quoted_decode(bytes),
        Transfer::Uuencode => uudecode(bytes),
    };
    bytes.into_iter().map(u32::from).collect()
}

pub fn encode(points: &[u32], transfer: Transfer) -> Vec<u8> {
    let bytes = || points.iter().map(|&point| point as u8).collect::<Vec<u8>>();
    match transfer {
        Transfer::Base64 => base64::engine::general_purpose::STANDARD
            .encode(bytes())
            .as_bytes()
            .chunks(BASE64_LINE)
            .collect::<Vec<_>>()
            .join(&b"\r\n"[..]),
        Transfer::Uuencode => uuencode(&bytes()),
        Transfer::QuotedPrintable => quoted_encode(points),
        Transfer::HtmlEntities => html_encode(points),
    }
}

/// Unlike `base64_decode`, PHP keeps going past bad characters, and each
/// counts as one bad character.
fn base64_decode(bytes: &[u8]) -> Vec<u32> {
    let mut points = Vec::new();
    let mut bits = 0;
    let mut cache = 0u32;
    for &byte in bytes {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'\r' | b'\n' | b' ' | b'\t' | b'=' => continue,
            _ => {
                points.push(BAD);
                continue;
            }
        };
        bits += 6;
        cache = (cache << 6) | u32::from(value);
        if bits == 24 {
            points.extend([(cache >> 16) & 0xFF, (cache >> 8) & 0xFF, cache & 0xFF]);
            cache = 0;
            bits = 0;
        }
    }
    match bits {
        18 => points.extend([(cache >> 10) & 0xFF, (cache >> 2) & 0xFF]),
        12 => points.push((cache >> 4) & 0xFF),
        _ => {}
    }
    points
}

fn quoted_decode(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'=' {
            // A soft line break.
            if bytes.get(index + 1) == Some(&b'\n') {
                index += 2;
                continue;
            }
            if bytes.get(index + 1..index + 3) == Some(b"\r\n") {
                index += 3;
                continue;
            }
            if let Some(byte) = bytes.get(index + 1..index + 3).and_then(hex_byte) {
                output.push(byte);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    output
}

/// Two hexadecimal digits as a byte.
pub fn hex_byte(digits: &[u8]) -> Option<u8> {
    u8::from_str_radix(std::str::from_utf8(digits).ok()?, 16).ok()
}

fn quoted_encode(points: &[u32]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut column = 0;
    for &point in points {
        match point {
            0 => {
                output.push(0);
                column = 0;
                continue;
            }
            0x0A => {
                output.extend_from_slice(b"\r\n");
                column = 0;
                continue;
            }
            0x0D => continue,
            _ => {}
        }
        if column >= QUOTED_LINE {
            output.extend_from_slice(b"=\r\n");
            column = 0;
        }
        if point >= 0x80 || point == u32::from(b'=') {
            output.extend_from_slice(format!("={:02X}", point & 0xFF).as_bytes());
            column += 3;
        } else {
            output.push(point as u8);
            column += 1;
        }
    }
    output
}

/// PHP writes a fixed header and no `end` line.
fn uuencode(bytes: &[u8]) -> Vec<u8> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let mut output = b"begin 0644 filename\n".to_vec();
    let digit = |n: u8| if n == 0 { b'`' } else { n + 32 };
    for line in bytes.chunks(UUENCODE_LINE) {
        output.push(digit(line.len() as u8));
        for group in line.chunks(3) {
            let a = group[0];
            let b = group.get(1).copied().unwrap_or(0);
            let c = group.get(2).copied().unwrap_or(0);
            output.extend([
                digit(a >> 2),
                digit(((a << 4) | (b >> 4)) & 0x3F),
                digit(((b << 2) | (c >> 6)) & 0x3F),
                digit(c & 0x3F),
            ]);
        }
        output.push(b'\n');
    }
    output
}

fn uudecode(bytes: &[u8]) -> Vec<u8> {
    if !bytes.starts_with(b"begin ") {
        return Vec::new();
    }
    let mut output = Vec::new();
    for line in bytes.split(|&byte| byte == b'\n').skip(1) {
        let Some(&length) = line.first() else {
            continue;
        };
        let length = usize::from(length.wrapping_sub(32) & 0x3F);
        if length == 0 || line == b"end" {
            break;
        }
        let mut decoded = Vec::new();
        for group in line[1..].as_chunks::<4>().0 {
            let [a, b, c, d] = group.map(|byte| byte.wrapping_sub(32) & 0x3F);
            decoded.extend([a << 2 | b >> 4, b << 4 | c >> 2, c << 6 | d]);
        }
        output.extend_from_slice(&decoded[..length.min(decoded.len())]);
    }
    output
}

/// PHP's HTML 4.01 entity names, from `get_html_translation_table()`.
fn html_names() -> &'static [(u32, &'static str)] {
    static NAMES: OnceLock<Vec<(u32, &'static str)>> = OnceLock::new();
    NAMES.get_or_init(|| {
        include_str!("../../data/entities.txt")
            .lines()
            .filter_map(|line| {
                let (point, name) = line.split_once(' ')?;
                Some((point.parse().ok()?, name))
            })
            .collect()
    })
}

fn html_encode(points: &[u32]) -> Vec<u8> {
    let mut output = String::new();
    for &point in points {
        if point < 0x80 {
            output.push(char::from(point as u8));
        } else if let Some((_, name)) = html_names().iter().find(|(named, _)| *named == point) {
            output.push_str(name);
        } else {
            output.push_str(&format!("&#{point};"));
        }
    }
    output.into_bytes()
}

/// Named, decimal and hexadecimal references; anything else stays as it is.
fn html_decode(bytes: &[u8]) -> Vec<u32> {
    let mut points = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'&'
            && let Some(length) = bytes[index..].iter().position(|&byte| byte == b';')
            && let Ok(reference) = std::str::from_utf8(&bytes[index..=index + length])
            && let Some(point) = html_reference(reference)
        {
            points.push(point);
            index += length + 1;
            continue;
        }
        points.push(u32::from(bytes[index]));
        index += 1;
    }
    points
}

/// The code point of `&name;`, `&#233;` or `&#xE9;`.
fn html_reference(reference: &str) -> Option<u32> {
    if let Some(digits) = reference
        .strip_prefix("&#x")
        .or_else(|| reference.strip_prefix("&#X"))
    {
        u32::from_str_radix(digits.trim_end_matches(';'), 16).ok()
    } else if let Some(digits) = reference.strip_prefix("&#") {
        digits.trim_end_matches(';').parse().ok()
    } else {
        html_names()
            .iter()
            .find(|(_, name)| *name == reference)
            .map(|(point, _)| *point)
    }
}
