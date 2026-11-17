//! HTML special-character conversion with document-specific entity validation.

use phera_extension_api::{
    Context, FunctionEntry, NativeResult, Param, ParamType, Registry, Value, args,
};

use super::{buffer, bytes};

mod options;

pub(super) use options::Document;
use options::{CharacterPolicy, InvalidInput, Options};

const ENCODE_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("flags", ParamType::Int).optional(),
    Param::new("encoding", ParamType::String)
        .nullable()
        .optional(),
    Param::new("double_encode", ParamType::Bool).optional(),
];

const DECODE_PARAMS: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("flags", ParamType::Int).optional(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync(
        "htmlspecialchars",
        ENCODE_PARAMS,
        encode,
    ));
    registry.function(FunctionEntry::sync(
        "htmlspecialchars_decode",
        DECODE_PARAMS,
        decode,
    ));
}

/// Numeric entities are parsed without allowing whitespace or signed values.
fn numeric(entity: &[u8]) -> Option<u32> {
    let tail = entity.strip_prefix(b"#")?;
    let (radix, tail) = if tail.first().is_some_and(|byte| matches!(byte, b'x' | b'X')) {
        (16, tail.get(1..)?)
    } else {
        (10, tail)
    };
    if tail.is_empty() {
        return None;
    }
    let mut value = 0u32;
    for &byte in tail {
        let digit = super::url::unhex(byte)? as u32;
        if digit >= radix {
            return None;
        }
        value = value.checked_mul(radix)?.checked_add(digit)?;
    }
    (value <= 0x10ffff).then_some(value)
}

/// PHP's document types differ on C1 controls and Unicode noncharacters.
pub(super) fn allowed(code: u32, document: Document) -> bool {
    if matches!(code, 9 | 10 | 13) {
        return true;
    }
    match document {
        Document::Xml1 | Document::Xhtml => {
            (0x20..=0xd7ff).contains(&code)
                || (0xe000..=0x10ffff).contains(&code) && !matches!(code, 0xfffe | 0xffff)
        }
        Document::Html5 => {
            code == 12
                || (0x20..=0x7e).contains(&code)
                || (0xa0..=0xd7ff).contains(&code)
                || (0xe000..=0x10ffff).contains(&code)
                    && code & 0xffff < 0xfffe
                    && !(0xfdd0..=0xfdef).contains(&code)
        }
        Document::Html401 => {
            (0x20..=0x7e).contains(&code)
                || (0xa0..=0xd7ff).contains(&code)
                || (0xe000..=0x10ffff).contains(&code)
        }
    }
}

/// Only the five special characters are decoded; ordinary HTML entities remain.
fn special(entity: &[u8], document: Document) -> Option<u8> {
    let value = match entity {
        b"amp" => 38,
        b"lt" => 60,
        b"gt" => 62,
        b"quot" => 34,
        b"apos" if document != Document::Html401 => 39,
        _ => numeric(entity)?,
    };
    matches!(value, 34 | 38 | 39 | 60 | 62).then_some(value as u8)
}

/// Entity decoding does not validate or reinterpret unrelated non-UTF-8 bytes.
fn decode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let options = Options::from_flags(args::int(arguments, 1).unwrap_or(11));
    let mut result = buffer(string.len())?;
    let mut cursor = 0;
    while let Some(&byte) = string.get(cursor) {
        if byte == b'&'
            && let Some(tail) = string.get(cursor + 1..)
            && let Some(end) = tail.iter().position(|&byte| byte == b';')
            && let Some(entity) = tail.get(..end)
            && let Some(decoded) = special(entity, options.document)
            && options.quotes.includes(decoded)
        {
            result.push(decoded);
            cursor += end + 2;
            continue;
        }
        result.push(byte);
        cursor += 1;
    }
    Ok(bytes(result))
}

/// double_encode=false preserves valid named entities for the selected document.
fn valid_entity(entity: &[u8], options: Options) -> bool {
    let document = options.document;
    if let Some(code) = numeric(entity) {
        return options.characters == CharacterPolicy::Any
            || match document {
                Document::Html401 => true,
                Document::Html5 => {
                    (allowed(code, Document::Html5) || (0xd800..=0xdfff).contains(&code))
                        && code != 13
                }
                _ => allowed(code, document),
            };
    }
    if document == Document::Xml1 {
        return matches!(entity, b"amp" | b"lt" | b"gt" | b"quot" | b"apos");
    }
    if document == Document::Xhtml && entity == b"apos" {
        return true;
    }
    let names = if document == Document::Html5 {
        super::html_entities::HTML5
    } else {
        super::html_entities::HTML4
    };
    names
        .as_bytes()
        .split(|&byte| byte == b' ')
        .any(|name| name == entity)
}

/// Invalid UTF-8 is discarded, substituted, or empties the whole result by flags.
fn encode(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let options = Options::from_flags(args::int(arguments, 1).unwrap_or(11));
    if string.is_empty() {
        return Ok(bytes(Vec::new()));
    }
    let encoding = super::html_encoding::Encoding::resolve(context, args::bytes(arguments, 2));
    let replacement = encoding.replacement();
    let mut result = buffer(string.len().saturating_mul(9))?;
    let mut cursor = 0;
    while let Some(&byte) = string.get(cursor) {
        if byte == b'&'
            && !args::bool(arguments, 3).unwrap_or(true)
            && let Some(tail) = string.get(cursor + 1..)
            && let Some(end) = tail.iter().position(|&byte| byte == b';')
            && let Some(entity) = tail.get(..end)
            && valid_entity(entity, options)
        {
            result.extend_from_slice(string.get(cursor..cursor + end + 2).unwrap_or_default());
            cursor += end + 2;
            continue;
        }
        let (code, length, valid) = encoding.next(string.get(cursor..).unwrap_or_default());
        if !valid {
            if options.invalid == InvalidInput::Ignore {
            } else if options.invalid == InvalidInput::Substitute {
                result.extend_from_slice(replacement);
            } else {
                return Ok(bytes(Vec::new()));
            }
        } else {
            let escaped = escape_special(byte, options);
            if let Some(escaped) = escaped {
                result.extend_from_slice(escaped);
            } else if options.characters == CharacterPolicy::DocumentOnly
                && encoding.disallowed(code, options.document)
            {
                result.extend_from_slice(replacement);
            } else {
                result.extend_from_slice(string.get(cursor..cursor + length).unwrap_or_default());
            }
        }
        cursor += length;
    }
    Ok(bytes(result))
}

/// PHP consumes malformed multibyte groups together, including surrogate encodings.
pub(super) fn utf8_char(string: &[u8]) -> (u32, usize, bool) {
    let first = string.first().copied().unwrap_or(0);
    if first < 128 {
        return (u32::from(first), 1, true);
    }
    let (length, minimum, mask) = match first {
        0xc2..=0xdf => (2, 0x80, 31),
        0xe0..=0xef => (3, 0x800, 15),
        0xf0..=0xf4 => (4, 0x10000, 7),
        _ => (1, 0, 0),
    };
    if length == 1 {
        return (0, 1, false);
    }
    let trail = |byte: u8| byte & 0xc0 == 0x80;
    if string.len() < length
        || !string
            .iter()
            .skip(1)
            .take(length - 1)
            .all(|&byte| trail(byte))
    {
        let mut consumed = 1;
        while consumed < length
            && string
                .get(consumed)
                .is_some_and(|&byte| !(byte < 0x80 || (0xc2..=0xf4).contains(&byte)))
        {
            consumed += 1;
        }
        return (0, consumed, false);
    }
    let mut code = u32::from(first & mask);
    for &byte in string.iter().skip(1).take(length - 1) {
        code = (code << 6) | u32::from(byte & 63);
    }
    (
        code,
        length,
        code >= minimum && code <= 0x10ffff && !(0xd800..=0xdfff).contains(&code),
    )
}

/// PHP spells apostrophes numerically in HTML 4.01 and by name in other documents.
fn escape_special(byte: u8, options: Options) -> Option<&'static [u8]> {
    match byte {
        b'&' => Some(b"&amp;"),
        b'<' => Some(b"&lt;"),
        b'>' => Some(b"&gt;"),
        b'"' if options.quotes.includes(byte) => Some(b"&quot;"),
        b'\'' if options.quotes.includes(byte) => Some(if options.document == Document::Html401 {
            b"&#039;"
        } else {
            b"&apos;"
        }),
        _ => None,
    }
}
