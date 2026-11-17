//! RFC 2047 headers: `mb_decode_mimeheader('=?UTF-8?B?w6k=?=') === 'é'`.
use crate::{
    codec::{decode, encode},
    encoding::{self, Encoding},
    state::Settings,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use phera_extension_api::{Context, NativeResult, Value, args};
#[derive(Clone, Copy)]
pub enum Transfer {
    Base64,
    Quoted,
}
pub fn transfer(bytes: &[u8]) -> Transfer {
    if matches!(bytes.first(), Some(b'q' | b'Q')) {
        Transfer::Quoted
    } else {
        Transfer::Base64
    }
}
pub fn encoded(bytes: &[u8], transfer: Transfer) -> String {
    match transfer {
        Transfer::Base64 => STANDARD.encode(bytes),
        Transfer::Quoted => {
            let mut out = String::new();
            for b in bytes {
                if b.is_ascii_alphabetic() || matches!(b, b'!' | b'*' | b'+' | b'-' | b'/') {
                    out.push(char::from(*b));
                } else {
                    out.push_str(&format!("={b:02X}"));
                }
            }
            out
        }
    }
}
pub fn decoded(bytes: &[u8], transfer: Transfer) -> Option<Vec<u8>> {
    match transfer {
        Transfer::Base64 => STANDARD.decode(bytes).ok(),
        Transfer::Quoted => {
            let mut out = Vec::new();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'='
                    && i + 2 < bytes.len()
                    && let Ok(s) = std::str::from_utf8(&bytes[i + 1..i + 3])
                    && let Ok(n) = u8::from_str_radix(s, 16)
                {
                    out.push(n);
                    i += 3;
                    continue;
                }
                out.push(if bytes[i] == b'_' { b' ' } else { bytes[i] });
                i += 1;
            }
            Some(out)
        }
    }
}
pub fn encode_header(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let state = ctx.state().get_or_default::<Settings>().clone();
    let name = args::string(a, 1)
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| crate::info::mail_charset(state.language).into());
    let target = encoding::resolve(&name, "mb_encode_mimeheader", 2, "charset")?;
    let method = transfer(args::bytes(a, 2));
    let newline = args::string(a, 3)
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| "\r\n".into());
    let source = args::bytes(a, 0);
    let bad = source
        .iter()
        .position(|b| !matches!(b, 32..=126) || matches!(b, b'=' | b'?' | b'_'));
    let Some(bad) = bad else {
        if args::int(a, 4).unwrap_or(0).max(0) as usize + source.len() > 74 {
            let mut output = newline.into_bytes();
            output.push(b' ');
            output.extend_from_slice(source);
            return Ok(Value::from(output));
        }
        return Ok(Value::from(source));
    };
    let prefix = source[..bad]
        .iter()
        .rposition(|b| *b == b' ')
        .map_or(0, |i| i + 1);
    let mut output = source[..prefix].to_vec();
    let points = decode(&source[prefix..], state.internal);
    let label = target.mime_name();
    let mut bytes = Vec::new();
    let mut column = prefix + args::int(a, 4).unwrap_or(0).max(0) as usize;
    if column + label.len() + 11 > 74 {
        output.extend_from_slice(newline.as_bytes());
        output.push(b' ');
        column = 1;
    }
    for cp in points {
        let next = encode(&[cp], target, &state.substitute);
        let mut candidate = bytes.clone();
        candidate.extend_from_slice(&next);
        if !bytes.is_empty() && column + label.len() + 7 + encoded(&candidate, method).len() > 74 {
            output.extend_from_slice(word(&bytes, label, method).as_bytes());
            output.extend_from_slice(newline.as_bytes());
            output.push(b' ');
            column = 1;
            bytes.clear();
        }
        bytes.extend_from_slice(&next);
    }
    output.extend_from_slice(word(&bytes, label, method).as_bytes());
    Ok(Value::from(output))
}
pub fn word(bytes: &[u8], label: &str, method: Transfer) -> String {
    format!(
        "=?{label}?{}?{}?=",
        match method {
            Transfer::Base64 => "B",
            Transfer::Quoted => "Q",
        },
        encoded(bytes, method)
    )
}
pub fn decode_header(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let target = ctx.state().get_or_default::<Settings>().internal;
    let source = args::bytes(a, 0);
    let mut unfolded = Vec::new();
    let mut i = 0;
    while i < source.len() {
        if matches!(source[i], b'\r' | b'\n') {
            while i < source.len() && source[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < source.len() {
                unfolded.push(b' ');
            }
        } else {
            unfolded.push(source[i]);
            i += 1;
        }
    }
    let points = decode_words(&unfolded);
    crate::text::result(ctx, &points, target)
}
pub fn decode_words(bytes: &[u8]) -> Vec<u32> {
    let mut output = Vec::new();
    let mut i = 0;
    let mut previous_word = false;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"=?") {
            let tail = &bytes[i + 2..];
            let word = (|| {
                let label_end = tail.iter().position(|b| *b == b'?')?;
                let label = std::str::from_utf8(&tail[..label_end]).ok()?;
                let encoding = Encoding::find(label)?;
                let rest = &tail[label_end + 1..];
                if rest.get(1) != Some(&b'?') {
                    return None;
                }
                let end = rest[2..].windows(2).position(|w| w == b"?=")? + 2;
                let decoded = decoded(&rest[2..end], transfer(rest))?;
                Some((decode(&decoded, encoding), 2 + label_end + 1 + end + 2))
            })();
            if let Some((points, length)) = word {
                output.extend(points);
                i += length;
                previous_word = true;
                continue;
            }
        }
        if previous_word && bytes[i].is_ascii_whitespace() {
            let end = bytes[i..]
                .iter()
                .position(|b| !b.is_ascii_whitespace())
                .map_or(bytes.len(), |n| i + n);
            if bytes[end..].starts_with(b"=?") {
                i = end;
                continue;
            }
        }
        output.push(u32::from(bytes[i]));
        i += 1;
        previous_word = false;
    }
    output
}
