//! Mail header words in RFC 2047's `=?charset?B?...?=` form:
//!
//! ```php
//! mb_encode_mimeheader('Café', 'UTF-8', 'B');   // "=?UTF-8?B?Q2Fmw6k=?="
//! mb_encode_mimeheader('Café', 'UTF-8', 'Q');   // "=?UTF-8?Q?Caf=C3=A9?="
//! mb_decode_mimeheader('=?UTF-8?B?w6k=?=');     // "é"
//! ```
//!
//! The B and Q schemes here serve `iconv_mime_*` as well.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

use crate::codec::{decode, encode, hex_byte};
use crate::encoding::{self, Encoding};
use crate::state::Settings;
use crate::text::encode_result;

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "mb_encode_mimeheader",
        MB_ENCODE_MIMEHEADER,
        mb_encode_mimeheader,
    ));
    registry.function(FunctionEntry::sync(
        "mb_decode_mimeheader",
        MB_DECODE_MIMEHEADER,
        mb_decode_mimeheader,
    ));
}

/// How the bytes of an encoded word are written.
#[derive(Clone, Copy)]
pub enum Scheme {
    Base64,
    /// Q: quoted-printable with `_` for a space.
    Quoted,
}

impl Scheme {
    /// `Q` or `q` is Q; anything else, even nothing, is B.
    pub fn parse(name: &[u8]) -> Self {
        if matches!(name.first(), Some(b'q' | b'Q')) {
            Self::Quoted
        } else {
            Self::Base64
        }
    }

    pub fn encode(self, bytes: &[u8]) -> String {
        match self {
            Self::Base64 => STANDARD.encode(bytes),
            Self::Quoted => {
                let mut encoded = String::new();
                for &byte in bytes {
                    if byte.is_ascii_alphabetic()
                        || matches!(byte, b'!' | b'*' | b'+' | b'-' | b'/')
                    {
                        encoded.push(char::from(byte));
                    } else {
                        encoded.push_str(&format!("={byte:02X}"));
                    }
                }
                encoded
            }
        }
    }

    pub fn decode(self, text: &[u8]) -> Option<Vec<u8>> {
        match self {
            Self::Base64 => STANDARD.decode(text).ok(),
            Self::Quoted => {
                let mut decoded = Vec::new();
                let mut index = 0;
                while index < text.len() {
                    if text[index] == b'='
                        && let Some(byte) = text.get(index + 1..index + 3).and_then(hex_byte)
                    {
                        decoded.push(byte);
                        index += 3;
                        continue;
                    }
                    decoded.push(if text[index] == b'_' {
                        b' '
                    } else {
                        text[index]
                    });
                    index += 1;
                }
                Some(decoded)
            }
        }
    }

    /// `=?charset?B?...?=`
    pub fn word(self, bytes: &[u8], charset: &str) -> String {
        let letter = match self {
            Self::Base64 => "B",
            Self::Quoted => "Q",
        };
        format!("=?{charset}?{letter}?{}?=", self.encode(bytes))
    }
}

/// A header line is at most this long, folding included.
const LINE_LENGTH: usize = 74;

const MB_ENCODE_MIMEHEADER: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("charset", ParamType::String)
        .nullable()
        .optional(),
    Param::new("transfer_encoding", ParamType::String)
        .nullable()
        .optional(),
    Param::new("newline", ParamType::String).optional(),
    Param::new("indent", ParamType::Int).optional(),
];

/// `mb_encode_mimeheader(string $string, ?string $charset = null,
/// ?string $transfer_encoding = null, string $newline = "\r\n", int $indent = 0): string`:
/// plain ASCII stays as it is; from the word with the first other byte on,
/// the rest is encoded and folded into lines.
fn mb_encode_mimeheader(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let settings = ctx.state().get_or_default::<Settings>().clone();
    // Without a charset or a scheme, the language's mail defaults decide.
    let mail = settings.language.mail();
    let charset =
        args::string(a, 1).map_or_else(|| mail.charset.into(), |name| name.to_string_lossy());
    let target = encoding::resolve(&charset, "mb_encode_mimeheader", 2, "charset")?;
    // "Quoted-Printable" is Q, "BASE64" is B.
    let scheme = Scheme::parse(
        args::string(a, 2).map_or(mail.header_encoding.as_bytes(), |scheme| scheme.as_bytes()),
    );
    let newline = args::string(a, 3).map_or(&b"\r\n"[..], |newline| newline.as_bytes());
    let indent = args::int(a, 4).unwrap_or(0).max(0) as usize;
    Ok(Value::from(encode_header(
        args::bytes(a, 0),
        &settings,
        target,
        scheme,
        newline,
        indent,
    )))
}

/// Mail and mb_encode_mimeheader both encode and fold MIME header words.
pub(crate) fn encode_header(
    source: &[u8],
    settings: &Settings,
    target: Encoding,
    scheme: Scheme,
    newline: &[u8],
    indent: usize,
) -> Vec<u8> {
    let needs_encoding =
        |byte: &u8| !matches!(byte, 32..=126) || matches!(byte, b'=' | b'?' | b'_');
    let Some(first_special) = source.iter().position(needs_encoding) else {
        if indent + source.len() > LINE_LENGTH {
            let mut output = newline.to_vec();
            output.push(b' ');
            output.extend_from_slice(source);
            return output;
        }
        return source.to_vec();
    };
    // The plain words before that byte stay as they are.
    let plain = source[..first_special]
        .iter()
        .rposition(|&byte| byte == b' ')
        .map_or(0, |space| space + 1);
    let mut output = source[..plain].to_vec();
    let label = target.mime_name();
    // `=?`, `?B?` and `?=` around the label and the text.
    let overhead = label.len() + 7;
    let mut column = plain + indent;
    if column + overhead + 4 > LINE_LENGTH {
        output.extend_from_slice(newline);
        output.push(b' ');
        column = 1;
    }
    let mut pending = Vec::new();
    for point in decode(&source[plain..], settings.internal) {
        let character = encode(&[point], target, &settings.substitute);
        let mut candidate = pending.clone();
        candidate.extend_from_slice(&character);
        if !pending.is_empty() && column + overhead + scheme.encode(&candidate).len() > LINE_LENGTH
        {
            output.extend_from_slice(scheme.word(&pending, label).as_bytes());
            output.extend_from_slice(newline);
            output.push(b' ');
            column = 1;
            pending.clear();
        }
        pending.extend_from_slice(&character);
    }
    output.extend_from_slice(scheme.word(&pending, label).as_bytes());
    output
}

const MB_DECODE_MIMEHEADER: &[Param] = &[Param::new("string", ParamType::String)];

/// `mb_decode_mimeheader(string $string): string`: into the internal
/// encoding, with folded lines joined and the space between two encoded
/// words dropped.
fn mb_decode_mimeheader(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let target = ctx.state().get_or_default::<Settings>().internal;
    let points = decode_header(args::bytes(a, 0));
    Ok(Value::from(encode_result(ctx, &points, target)))
}

fn is_line_break(byte: u8) -> bool {
    matches!(byte, b'\r' | b'\n')
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, b'\r' | b'\n' | b'\t' | b' ')
}

/// The code points of a header, as PHP's `mb_mime_header_decode` reads it:
///
/// - after an encoded word, a run of whitespace and line breaks becomes one
///   space, written only if something other than another encoded word
///   follows;
/// - plain text runs to the next `=` or line break, as ASCII, so a byte from
///   0x80 on is `?`;
/// - a run of line breaks and whitespace becomes one space, except at the
///   very end.
fn decode_header(bytes: &[u8]) -> Vec<u32> {
    let mut points = Vec::with_capacity(bytes.len());
    let mut index = 0;
    let mut space_pending = false;
    let whitespace_from = |index: usize| {
        bytes[index..]
            .iter()
            .take_while(|&&byte| is_whitespace(byte))
            .count()
    };
    while index < bytes.len() {
        if let Some((decoded, length)) = encoded_word(&bytes[index..]) {
            points.extend(decoded);
            index += length;
            let run = whitespace_from(index);
            space_pending = run > 0;
            index += run;
            continue;
        }
        if space_pending {
            points.push(u32::from(b' '));
            space_pending = false;
        }
        if !is_line_break(bytes[index]) {
            let run = 1 + bytes[index + 1..]
                .iter()
                .take_while(|&&byte| byte != b'=' && !is_line_break(byte))
                .count();
            let plain = &bytes[index..index + run];
            points.extend(plain.iter().map(|&byte| {
                if byte < 0x80 {
                    u32::from(byte)
                } else {
                    u32::from(b'?')
                }
            }));
            index += run;
        }
        if index < bytes.len() && is_line_break(bytes[index]) {
            index += whitespace_from(index);
            if index < bytes.len() {
                points.push(u32::from(b' '));
            }
        }
    }
    points
}

/// The decoded code points of an encoded word at the start of `bytes`, and
/// its length; `None` if it is not one or names an unknown charset.
fn encoded_word(bytes: &[u8]) -> Option<(Vec<u32>, usize)> {
    let rest = bytes.strip_prefix(b"=?")?;
    let label_end = rest.iter().position(|&byte| byte == b'?')?;
    let encoding = Encoding::find(std::str::from_utf8(&rest[..label_end]).ok()?)?;
    let rest = &rest[label_end + 1..];
    if rest.get(1) != Some(&b'?') {
        return None;
    }
    let end = rest[2..].windows(2).position(|pair| pair == b"?=")? + 2;
    let decoded = Scheme::parse(rest).decode(&rest[2..end])?;
    Some((decode(&decoded, encoding), 2 + label_end + 1 + end + 2))
}
