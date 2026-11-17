//! Transfer encodings are deprecated: `mb_convert_encoding('abc', 'BASE64') === 'YWJj'`.
use crate::{encoding::Encoding, state::Settings};
use base64::{
    Engine, alphabet,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};
use phera_extension_api::Context;

pub fn selected(ctx: &mut dyn Context, name: &str, enc: Encoding, function: &str) {
    let slot = &mut ctx.state().get_or_default::<Settings>().last_encoding;
    if slot.as_ref().is_some_and(|s| s.eq_ignore_ascii_case(name)) {
        return;
    }
    *slot = Some(name.into());
    let message = match enc.0 {
        44 => "Handling Base64 via mbstring is deprecated; use base64_encode/base64_decode instead",
        45 => {
            "Handling Uuencode via mbstring is deprecated; use convert_uuencode/convert_uudecode instead"
        }
        46 => concat!(
            "Handling HTML entities via mbstring is deprecated; use htmlspecialchars, ",
            "htmlentities, or mb_encode_numericentity/mb_decode_numericentity instead"
        ),
        47 => concat!(
            "Handling QPrint via mbstring is deprecated; use ",
            "quoted_printable_encode/quoted_printable_decode instead"
        ),
        _ => return,
    };
    ctx.deprecated(format!("{function}(): {message}"));
}
pub fn decode(bytes: &[u8], enc: Encoding) -> Vec<u32> {
    let out = match enc.0 {
        44 => {
            let clean: Vec<_> = bytes
                .iter()
                .copied()
                .filter(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/'))
                .collect();
            let engine = GeneralPurpose::new(
                &alphabet::STANDARD,
                GeneralPurposeConfig::new()
                    .with_decode_padding_mode(DecodePaddingMode::Indifferent)
                    .with_decode_allow_trailing_bits(true),
            );
            engine
                .decode(&clean[..clean.len() - usize::from(clean.len() % 4 == 1)])
                .unwrap_or_default()
        }
        47 => {
            let mut out = Vec::new();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'=' {
                    if bytes.get(i + 1) == Some(&b'\n') {
                        i += 2;
                        continue;
                    }
                    if bytes.get(i + 1..i + 3) == Some(b"\r\n") {
                        i += 3;
                        continue;
                    }
                    if let Some(s) = bytes
                        .get(i + 1..i + 3)
                        .and_then(|b| std::str::from_utf8(b).ok())
                        && let Ok(n) = u8::from_str_radix(s, 16)
                    {
                        out.push(n);
                        i += 3;
                        continue;
                    }
                }
                out.push(bytes[i]);
                i += 1;
            }
            out
        }
        45 => uudecode(bytes),
        _ => return html_decode(bytes),
    };
    out.into_iter().map(u32::from).collect()
}
pub fn encode(points: &[u32], enc: Encoding) -> Vec<u8> {
    let bytes: Vec<_> = points.iter().map(|cp| *cp as u8).collect();
    match enc.0 {
        44 => base64::engine::general_purpose::STANDARD
            .encode(&bytes)
            .into_bytes(),
        47 => {
            let mut out = String::new();
            for b in bytes {
                if matches!(b, 33..=60 | 62..=126 | 9 | 32 | 10 | 13) {
                    out.push(char::from(b));
                } else {
                    out.push_str(&format!("={b:02X}"));
                }
            }
            out.into_bytes()
        }
        45 => uuencode(&bytes),
        _ => {
            let mut out = String::new();
            for cp in points {
                if *cp < 128 {
                    out.push(char::from(*cp as u8));
                } else if let Some(name) = html_names().iter().find(|(c, _)| c == cp) {
                    out.push_str(name.1);
                } else {
                    out.push_str(&format!("&#{cp};"));
                }
            }
            out.into_bytes()
        }
    }
}
fn uuencode(bytes: &[u8]) -> Vec<u8> {
    if bytes.is_empty() {
        return Vec::new();
    }
    let mut out = b"begin 0644 filename\n".to_vec();
    let digit = |n: u8| if n == 0 { b'`' } else { n + 32 };
    for line in bytes.chunks(45) {
        out.push(digit(line.len() as u8));
        for group in line.chunks(3) {
            let a = group[0];
            let b = group.get(1).copied().unwrap_or(0);
            let c = group.get(2).copied().unwrap_or(0);
            out.extend([
                digit(a >> 2),
                digit(((a << 4) | (b >> 4)) & 63),
                digit(((b << 2) | (c >> 6)) & 63),
                digit(c & 63),
            ]);
        }
        out.push(b'\n');
    }
    out
}
fn uudecode(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in bytes.split(|b| *b == b'\n').skip(1) {
        let Some(length) = line.first() else {
            continue;
        };
        let length = usize::from(length.wrapping_sub(32) & 63);
        let mut decoded = Vec::new();
        for group in line[1..].as_chunks::<4>().0.iter() {
            let n: Vec<_> = group.iter().map(|b| b.wrapping_sub(32) & 63).collect();
            decoded.extend([
                n[0] << 2 | n[1] >> 4,
                n[1] << 4 | n[2] >> 2,
                n[2] << 6 | n[3],
            ]);
        }
        out.extend_from_slice(&decoded[..length.min(decoded.len())]);
    }
    out
}
fn html_names() -> &'static Vec<(u32, &'static str)> {
    static NAMES: std::sync::OnceLock<Vec<(u32, &'static str)>> = std::sync::OnceLock::new();
    NAMES.get_or_init(|| {
        include_str!("../data/entities.txt")
            .lines()
            .filter_map(|line| {
                let (cp, name) = line.split_once(' ')?;
                Some((cp.parse().ok()?, name))
            })
            .collect()
    })
}
fn html_decode(bytes: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&'
            && let Some(length) = bytes[i..].iter().position(|b| *b == b';')
            && let Ok(name) = std::str::from_utf8(&bytes[i..=i + length])
        {
            let cp = if let Some(n) = name
                .strip_prefix("&#x")
                .or_else(|| name.strip_prefix("&#X"))
            {
                u32::from_str_radix(n.trim_end_matches(';'), 16).ok()
            } else if let Some(n) = name.strip_prefix("&#") {
                n.trim_end_matches(';').parse().ok()
            } else {
                html_names()
                    .iter()
                    .find(|(_, n)| *n == name)
                    .map(|(cp, _)| *cp)
            };
            if let Some(cp) = cp {
                out.push(cp);
                i += length + 1;
                continue;
            }
        }
        out.push(u32::from(bytes[i]));
        i += 1;
    }
    out
}
