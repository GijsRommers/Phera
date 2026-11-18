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

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

use super::convert::{Failure, convert, report, try_convert};
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
    let Some(converted) = convert(ctx, args::bytes(a, 1), &input, &output, "iconv_mime_encode")
    else {
        return Ok(Value::Bool(false));
    };
    let mut field = args::bytes(a, 0).to_vec();
    field.extend_from_slice(b": ");
    // `=?`, `?B?` and `?=` around the charset, after the field name.
    let overhead = output.len() + 7 + field.len();
    if overhead >= line_length {
        ctx.warning("iconv_mime_encode(): Buffer length exceeded".into());
        return Ok(Value::Bool(false));
    }
    let budget = line_length - overhead;
    let mut pending = Vec::new();
    for byte in converted {
        let mut candidate = pending.clone();
        candidate.push(byte);
        let too_long = scheme.encode(&candidate).len() > budget
            || matches!(scheme, Scheme::Base64)
                && candidate.len() > (budget / 4 * 3).saturating_sub(4);
        if !pending.is_empty() && too_long {
            field.extend_from_slice(scheme.word(&pending, &output).as_bytes());
            field.extend_from_slice(newline.as_bytes());
            field.push(b' ');
            pending.clear();
        }
        pending.push(byte);
    }
    field.extend_from_slice(scheme.word(&pending, &output).as_bytes());
    Ok(Value::from(field))
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

/// Joins folded lines: a line break followed by spaces or tabs becomes one
/// space, and a line break without them ends the header. Also says whether
/// the header ran to the end of the input, without such a line break.
fn unfold(bytes: &[u8]) -> (Vec<u8>, bool) {
    let mut unfolded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if !matches!(bytes[index], b'\r' | b'\n') {
            unfolded.push(bytes[index]);
            index += 1;
            continue;
        }
        if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            index += 1;
        }
        index += 1;
        if !matches!(bytes.get(index), Some(b' ' | b'\t')) {
            return (unfolded, false);
        }
        while matches!(bytes.get(index), Some(b' ' | b'\t')) {
            index += 1;
        }
        while matches!(unfolded.last(), Some(b' ' | b'\t')) {
            unfolded.pop();
        }
        unfolded.push(b' ');
    }
    (unfolded, true)
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

/// The header with its encoded words converted to `target`, and the rest
/// converted from ASCII.
fn decode(
    ctx: &mut dyn Context,
    input: &[u8],
    target: &str,
    mode: &Mode,
    function: &str,
) -> Option<Vec<u8>> {
    let (input, ends_input) = unfold(input);
    let mut decoded = Vec::new();
    let mut index = 0;
    let mut after_word = false;
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
                        let at_end = ends_input
                            && end == input.len()
                            && failure != Failure::UnknownConversion;
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
        // Whitespace between two encoded words is dropped.
        if after_word && input[index].is_ascii_whitespace() {
            let next = input[index..]
                .iter()
                .position(|byte| !byte.is_ascii_whitespace())
                .map_or(input.len(), |offset| index + offset);
            if word(&input[next..]).is_some() {
                index = next;
                continue;
            }
        }
        decoded.extend(convert(
            ctx,
            &input[index..index + 1],
            "ASCII",
            target,
            function,
        )?);
        index += 1;
        after_word = false;
    }
    Some(decoded)
}
