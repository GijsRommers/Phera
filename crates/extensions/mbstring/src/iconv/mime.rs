//! iconv's mail header functions, which take a whole field:
//!
//! ```php
//! iconv_mime_encode('Subject', 'Café');           // "Subject: =?UTF-8?B?Q2Fmw6k=?="
//! iconv_mime_decode('Subject: =?UTF-8?Q?Caf=C3=A9?='); // "Subject: Café"
//! iconv_mime_decode_headers("A: 1\r\nA: 2\r\n");   // ['A' => ['1', '2']]
//! ```
//!
//! The words are converted by the platform's iconv, the B and Q schemes are
//! [`crate::mime::Scheme`].

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

use super::convert::{Failure, code_points, convert, report, try_convert};
use super::settings::{encoding_argument, internal_encoding};
use crate::mime::Scheme;

pub fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "iconv_mime_encode",
        ICONV_MIME_ENCODE,
        iconv_mime_encode,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_mime_decode",
        ICONV_MIME_DECODE,
        iconv_mime_decode,
    ));
    registry.function(FunctionEntry::sync(
        "iconv_mime_decode_headers",
        ICONV_MIME_DECODE_HEADERS,
        iconv_mime_decode_headers,
    ));
}

/// `ICONV_MIME_DECODE_STRICT`: decode only words that stand alone,
/// between whitespace.
pub const DECODE_STRICT: i64 = 1;
/// `ICONV_MIME_DECODE_CONTINUE_ON_ERROR`: keep a word that cannot be
/// decoded as it is, instead of failing.
pub const DECODE_CONTINUE_ON_ERROR: i64 = 2;

/// The `$mode` flags.
struct Mode {
    strict: bool,
    continue_on_error: bool,
}

impl Mode {
    fn from_flags(flags: i64) -> Self {
        Self {
            strict: flags & DECODE_STRICT != 0,
            continue_on_error: flags & DECODE_CONTINUE_ON_ERROR != 0,
        }
    }
}

const ICONV_MIME_ENCODE: &[Param] = &[
    Param::new("field_name", ParamType::String),
    Param::new("field_value", ParamType::String),
    Param::new("options", ParamType::Array).optional(),
];

/// `iconv_mime_encode(string $field_name, string $field_value, array $options = []): string|false`.
/// The options are `scheme`, `input-charset`, `output-charset`,
/// `line-length` and `line-break-chars`.
fn iconv_mime_encode(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let function = "iconv_mime_encode";
    let options = args::array(a, 2);
    let option = |key: &str| options.and_then(|options| options.get(&ArrayKey::str(key)));
    let input = option("input-charset").map_or_else(|| internal_encoding(ctx), crate::string_of);
    let output = option("output-charset").map_or_else(|| input.clone(), crate::string_of);
    let scheme = option("scheme").map_or_else(|| "B".into(), crate::string_of);
    let scheme = Scheme::parse(scheme.as_bytes());
    let line_length = match option("line-length") {
        Some(Value::Int(length)) => (*length).max(0) as usize,
        _ => 76,
    };
    let newline = option("line-break-chars").map_or_else(|| "\r\n".into(), crate::string_of);
    // PHP checks the lengths before it converts anything.
    let name = args::bytes(a, 0);
    if name.len() + 2 >= line_length || output.len() + 12 >= line_length {
        ctx.warning(format!("{function}(): Buffer length exceeded"));
        return Ok(Value::Bool(false));
    }
    let value = args::bytes(a, 1);
    let Some(converted) = convert(ctx, value, &input, &output, function) else {
        return Ok(Value::Bool(false));
    };
    let characters = output_characters(ctx, value, &input, &output, &converted);
    let pieces: Vec<&[u8]> = match &characters {
        Some(characters) => characters.iter().map(Vec::as_slice).collect(),
        None => converted.chunks(1).collect(),
    };
    let header = Header {
        name,
        charset: &output,
        scheme,
        line_length,
        newline: newline.as_bytes(),
    };
    match header.encode(&pieces) {
        Ok(field) => Ok(Value::from(field)),
        Err(EncodeError::TooBig) => {
            ctx.warning(format!("{function}(): Buffer length exceeded"));
            Ok(Value::Bool(false))
        }
        Err(EncodeError::NoRoom) => {
            // PHP's "unknown error" with `errno` E2BIG, 7 on Linux and macOS.
            ctx.notice(format!("{function}(): Unknown error (7)"));
            Ok(Value::Bool(false))
        }
    }
}

/// The value as the bytes of each of its characters in the output charset,
/// which PHP never splits between two encoded words. Each distinct
/// character is converted once. A stateful charset (ISO-2022-JP, UTF-16
/// with its byte order mark) converts differently character by character;
/// for those this is `None`, and the bytes are split anywhere.
fn output_characters(
    ctx: &mut dyn Context,
    value: &[u8],
    input: &str,
    output: &str,
    converted: &[u8],
) -> Option<Vec<Vec<u8>>> {
    let points = code_points(ctx, value, input, "iconv_mime_encode")?;
    let mut known: HashMap<u32, Vec<u8>> = HashMap::new();
    let mut characters = Vec::with_capacity(points.len());
    for point in points {
        let bytes = match known.entry(point) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                entry.insert(iconv_native::convert(point.to_le_bytes(), "UCS-4LE", output).ok()?)
            }
        };
        characters.push(bytes.clone());
    }
    let whole: Vec<u8> = characters.concat();
    (whole == converted).then_some(characters)
}

enum EncodeError {
    /// PHP's "Buffer length exceeded".
    TooBig,
    /// No character fits a Q word.
    NoRoom,
}

/// PHP's `_php_iconv_mime_encode`: encoded words filled up to the line
/// length, each line after the first starting with the newline and a space.
struct Header<'a> {
    name: &'a [u8],
    charset: &'a str,
    scheme: Scheme,
    line_length: usize,
    newline: &'a [u8],
}

impl Header<'_> {
    fn encode(&self, pieces: &[&[u8]]) -> Result<Vec<u8>, EncodeError> {
        let mut field = self.name.to_vec();
        field.extend_from_slice(b": ");
        // What is left of the current line.
        let mut room = self.line_length - self.name.len() - 2;
        // `=??X??=` around the charset, and the shortest encoded text.
        let shortest = 7
            + self.charset.len()
            + if matches!(self.scheme, Scheme::Base64) {
                4
            } else {
                3
            };
        let mut next = 0;
        loop {
            if room < shortest + self.newline.len() + 1 {
                field.extend_from_slice(self.newline);
                field.push(b' ');
                room = self.line_length - 1;
            }
            field.extend_from_slice(format!("=?{}?", self.charset).as_bytes());
            room = room.saturating_sub(3 + self.charset.len());
            let (taken, text) = match self.scheme {
                Scheme::Base64 => {
                    field.extend_from_slice(b"B?");
                    room = room.saturating_sub(2);
                    // Three bytes make four characters; PHP keeps four bytes
                    // free for the shift sequence of a stateful charset.
                    let size = room.saturating_sub(2) / 4 * 3;
                    if size <= 4 {
                        return Err(EncodeError::TooBig);
                    }
                    let (taken, bytes) = take(pieces, next, size - 4);
                    if taken == 0 && next < pieces.len() {
                        return Err(EncodeError::TooBig);
                    }
                    (taken, STANDARD.encode(bytes))
                }
                Scheme::Quoted => {
                    field.extend_from_slice(b"Q?");
                    room = room.saturating_sub(2);
                    let limit = room.saturating_sub(2);
                    let mut size = limit;
                    // Fit the bytes, then shrink until their quoted form fits.
                    loop {
                        let (taken, bytes) = take(pieces, next, size);
                        if taken == 0 && next < pieces.len() {
                            return Err(EncodeError::NoRoom);
                        }
                        let required: usize = bytes.iter().map(|&byte| quoted_length(byte)).sum();
                        if required <= limit || size == 0 {
                            break (taken, quoted(&bytes));
                        }
                        size = size.saturating_sub((required - limit).div_ceil(3));
                    }
                }
            };
            next += taken;
            room = room.saturating_sub(text.len() + 2);
            field.extend_from_slice(text.as_bytes());
            field.extend_from_slice(b"?=");
            if next >= pieces.len() {
                return Ok(field);
            }
        }
    }
}

/// As many pieces from `start` as fit in `budget` bytes.
fn take(pieces: &[&[u8]], start: usize, budget: usize) -> (usize, Vec<u8>) {
    let mut bytes = Vec::new();
    let mut taken = 0;
    for piece in &pieces[start..] {
        if bytes.len() + piece.len() > budget {
            break;
        }
        bytes.extend_from_slice(piece);
        taken += 1;
    }
    (taken, bytes)
}

/// iconv's Q encoding writes printable ASCII as it is, except `=`, `?` and
/// `_`, and everything else, the space included, as `=XX`.
fn quoted_length(byte: u8) -> usize {
    if matches!(byte, 0x21..=0x7E) && !matches!(byte, b'=' | b'?' | b'_') {
        1
    } else {
        3
    }
}

fn quoted(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 3);
    for &byte in bytes {
        if quoted_length(byte) == 1 {
            text.push(char::from(byte));
        } else {
            text.push_str(&format!("={byte:02X}"));
        }
    }
    text
}

const ICONV_MIME_DECODE: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("mode", ParamType::Int).optional(),
    crate::ENCODING,
];

/// `iconv_mime_decode(string $string, int $mode = 0, ?string $encoding = null): string|false`
fn iconv_mime_decode(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let target = encoding_argument(ctx, a, 2);
    let mode = Mode::from_flags(args::int(a, 1).unwrap_or(0));
    let decoded = decode(ctx, args::bytes(a, 0), &target, &mode, "iconv_mime_decode");
    Ok(decoded.map_or(Value::Bool(false), Value::from))
}

const ICONV_MIME_DECODE_HEADERS: &[Param] = &[
    Param::new("headers", ParamType::String),
    Param::new("mode", ParamType::Int).optional(),
    crate::ENCODING,
];

/// `iconv_mime_decode_headers(string $headers, int $mode = 0, ?string $encoding = null): array|false`:
/// the fields up to the first empty line, a repeated field as a list. Each
/// field is decoded whole, name included, and then split at its colon.
fn iconv_mime_decode_headers(ctx: &mut dyn Context, a: &[Value]) -> NativeResult {
    let target = encoding_argument(ctx, a, 2);
    let mode = Mode::from_flags(args::int(a, 1).unwrap_or(0));
    let mut result = PhpArray::new();
    for field in fields(args::bytes(a, 0)) {
        let Some(decoded) = decode(ctx, field, &target, &mode, "iconv_mime_decode_headers") else {
            return Ok(Value::Bool(false));
        };
        let Some(colon) = decoded.iter().position(|&byte| byte == b':') else {
            continue;
        };
        let key = ArrayKey::str(&decoded[..colon]);
        let mut value = &decoded[colon + 1..];
        while let [b' ' | b'\t', rest @ ..] = value {
            value = rest;
        }
        let value = Value::from(value);
        let Some(previous) = result.get(&key).cloned() else {
            result.insert(key, value);
            continue;
        };
        let mut values = match previous {
            Value::Array(values) => (*values).clone(),
            single => {
                let mut values = PhpArray::new();
                values.push(single)?;
                values
            }
        };
        values.push(value)?;
        result.insert(key, Value::from(values));
    }
    Ok(Value::from(result))
}

/// The header fields before the first empty line, each with its
/// continuation lines (which start with a space or tab) and line breaks.
fn fields(headers: &[u8]) -> Vec<&[u8]> {
    let mut fields: Vec<std::ops::Range<usize>> = Vec::new();
    let mut offset = 0;
    for line in headers.split_inclusive(|&byte| byte == b'\n') {
        let range = offset..offset + line.len();
        offset = range.end;
        let content = line.strip_suffix(b"\n").unwrap_or(line);
        let content = content.strip_suffix(b"\r").unwrap_or(content);
        if content.is_empty() {
            break;
        }
        if matches!(content.first(), Some(b' ' | b'\t')) {
            if let Some(field) = fields.last_mut() {
                field.end = range.end;
            }
        } else {
            fields.push(range);
        }
    }
    fields.into_iter().map(|range| &headers[range]).collect()
}

/// An encoded word at the start of the input.
struct Word<'a> {
    /// Without RFC 2231's `*language`.
    charset: &'a str,
    bytes: Vec<u8>,
    /// Of the whole `=?...?=`.
    length: usize,
}

fn word(bytes: &[u8]) -> Option<Word<'_>> {
    let bytes = bytes.strip_prefix(b"=?")?;
    let end = bytes.iter().position(|&byte| byte == b'?')?;
    let charset = std::str::from_utf8(&bytes[..end]).ok()?;
    let tail = &bytes[end + 1..];
    if tail.get(1) != Some(&b'?') || !matches!(tail.first(), Some(b'B' | b'b' | b'Q' | b'q')) {
        return None;
    }
    let close = tail[2..].windows(2).position(|pair| pair == b"?=")? + 2;
    Some(Word {
        charset: charset.split('*').next()?,
        bytes: Scheme::parse(tail).decode(&tail[2..close])?,
        length: 2 + end + 1 + close + 2,
    })
}

/// A run of spaces, tabs and folded line breaks, and what it means.
struct Gap {
    end: usize,
    /// It contains a line break followed by a space or tab.
    folded: bool,
    /// It ends the header: a line break not followed by a space or tab, or
    /// the end of the input.
    ends_header: bool,
}

fn gap(input: &[u8], start: usize) -> Gap {
    let mut index = start;
    let mut folded = false;
    loop {
        match input.get(index) {
            Some(b' ' | b'\t') => index += 1,
            Some(b'\r') if input.get(index + 1) == Some(&b'\n') => {
                if !matches!(input.get(index + 2), Some(b' ' | b'\t')) {
                    break;
                }
                folded = true;
                index += 2;
            }
            Some(b'\n') => {
                if !matches!(input.get(index + 1), Some(b' ' | b'\t')) {
                    break;
                }
                folded = true;
                index += 1;
            }
            _ => break,
        }
    }
    let ends_header =
        matches!(input.get(index), None | Some(b'\n')) || input[index..].starts_with(b"\r\n");
    Gap {
        end: index,
        folded,
        ends_header,
    }
}

/// The header with its encoded words converted to `target`, and the rest
/// converted from ASCII, following PHP's `_php_iconv_mime_decode`:
///
/// - spaces and tabs are held back, and written only when plain text
///   follows, or an encoded word after plain text; at the end of the header
///   they are dropped;
/// - a folded line break, with the spaces around it, becomes one space, or
///   nothing right after an encoded word;
/// - a line break that is not folded ends the header.
fn decode(
    ctx: &mut dyn Context,
    input: &[u8],
    target: &str,
    mode: &Mode,
    function: &str,
) -> Option<Vec<u8>> {
    let mut decoded = Vec::new();
    let mut index = 0;
    let mut after_word = false;
    let ascii =
        |ctx: &mut dyn Context, bytes: &[u8]| convert(ctx, bytes, "ASCII", target, function);
    while index < input.len() {
        if let Some(word) = word(&input[index..]) {
            let end = index + word.length;
            let alone = (index == 0 || input[index - 1].is_ascii_whitespace())
                && (end == input.len() || input[end].is_ascii_whitespace());
            if !mode.strict || alone {
                match try_convert(&word.bytes, word.charset, target) {
                    Ok(bytes) => {
                        decoded.extend(bytes);
                        after_word = true;
                    }
                    Err(failure) if mode.continue_on_error => {
                        // PHP drops the last byte of an invalid word that
                        // ends the input, but not one before a line break.
                        let at_end = end == input.len() && failure != Failure::UnknownConversion;
                        decoded.extend_from_slice(&input[index..end - usize::from(at_end)]);
                        after_word = false;
                    }
                    Err(Failure::UnknownConversion) => {
                        ctx.warning(format!(
                            "{function}(): Wrong encoding, conversion from \"???\" to \"{target}\" \
                             is not allowed"
                        ));
                        return None;
                    }
                    Err(failure) => {
                        report(ctx, failure, word.charset, target, function);
                        return None;
                    }
                }
                index = end;
                continue;
            }
        }
        // A bare carriage return is plain text, as in PHP.
        let starts_gap =
            matches!(input[index], b' ' | b'\t' | b'\n') || input[index..].starts_with(b"\r\n");
        if starts_gap {
            let gap = gap(input, index);
            if gap.folded {
                if !after_word {
                    decoded.extend(ascii(ctx, b" ")?);
                }
            } else if !gap.ends_header {
                let before_word = word(&input[gap.end..]).is_some();
                if !(before_word && after_word) {
                    decoded.extend(ascii(ctx, &input[index..gap.end])?);
                }
            }
            if gap.ends_header {
                break;
            }
            index = gap.end;
            continue;
        }
        decoded.extend(ascii(ctx, &input[index..index + 1])?);
        index += 1;
        after_word = false;
    }
    Some(decoded)
}
