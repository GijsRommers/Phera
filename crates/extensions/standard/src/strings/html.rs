//! `htmlspecialchars("<a href='x'>")` is `"&lt;a href=&#039;x&#039;&gt;"`,
//! and `htmlspecialchars_decode` turns those five entities back.
//!
//! What the `$flags` select is parsed in [`options`]; the character
//! encodings and how PHP steps over invalid bytes in each are in
//! [`encoding`].

use phera_extension_api::{Context, FunctionEntry, NativeResult, Param, ParamType, Value, args};

use super::buffer;
use super::url::hex_digit;
use crate::constants::{ENT_HTML401, ENT_QUOTES, ENT_SUBSTITUTE};

mod decode;
mod encoding;
mod entities;
mod full;
mod legacy;
mod options;

use encoding::{Decoded, Encoding};
use options::{Document, InvalidInput, Options};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    HTMLSPECIALCHARS,
    full::HTMLENTITIES,
    HTMLSPECIALCHARS_DECODE,
    decode::HTML_ENTITY_DECODE,
    decode::GET_HTML_TRANSLATION_TABLE,
];

/// The `$flags` both functions default to.
const DEFAULT_FLAGS: i64 = ENT_QUOTES | ENT_SUBSTITUTE | ENT_HTML401;

/// `htmlspecialchars(string $string, int $flags = ENT_QUOTES |
/// ENT_SUBSTITUTE | ENT_HTML401, ?string $encoding = null, bool
/// $double_encode = true): string`, with `&`, `<`, `>` and the quotes the
/// flags name as entities.
///
/// Invalid bytes in the encoding (UTF-8 by default) are replaced by U+FFFD
/// with `ENT_SUBSTITUTE`, dropped with `ENT_IGNORE`, and otherwise make the
/// whole result empty. With `$double_encode` false, entities that are
/// already there stay as they are.
///
/// ```php
/// htmlspecialchars("<a href='x'>T&amp;C</a>");
/// // "&lt;a href=&#039;x&#039;&gt;T&amp;amp;C&lt;/a&gt;"
/// htmlspecialchars("T&amp;C", ENT_QUOTES, null, false); // "T&amp;C"
/// htmlspecialchars("\xff", ENT_QUOTES);                 // ""
/// ```
const HTMLSPECIALCHARS: FunctionEntry =
    FunctionEntry::sync("htmlspecialchars", ENCODE, htmlspecialchars);

const ENCODE: &[Param] = &[
    Param::new("string", ParamType::String),
    Param::new("flags", ParamType::Int)
        .with_default(phera_extension_api::ConstantValue::Int(DEFAULT_FLAGS)),
    Param::new("encoding", ParamType::String)
        .nullable()
        .optional(),
    Param::new("double_encode", ParamType::Bool)
        .with_default(phera_extension_api::ConstantValue::Bool(true)),
];

#[derive(Clone, Copy)]
enum SubstituteEntities {
    Special,
    All,
}

fn htmlspecialchars(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    encode(context, arguments, SubstituteEntities::Special)
}

fn encode(
    context: &mut dyn Context,
    arguments: &[Value],
    entities: SubstituteEntities,
) -> NativeResult {
    let function = match entities {
        SubstituteEntities::Special => "htmlspecialchars",
        SubstituteEntities::All => "htmlentities",
    };
    let string = args::bytes(arguments, 0);
    let options = Options::from_flags(args::int(arguments, 1).unwrap_or(DEFAULT_FLAGS));
    // An empty string returns before the encoding is looked at, so an
    // unknown one does not warn.
    if string.is_empty() {
        return Ok(Value::from(""));
    }
    let encoding = Encoding::resolve(context, function, args::bytes(arguments, 2));
    let mut full = matches!(entities, SubstituteEntities::All);
    if full
        && !matches!(
            encoding,
            Encoding::Utf8 | Encoding::Latin1 | Encoding::SingleByte(_)
        )
    {
        context.notice(
            "htmlentities(): Only basic entities substitution is supported for multi-byte encodings other than UTF-8; functionality is equivalent to htmlspecialchars".into()
        );
        full = false;
    }
    let table = full
        .then(|| self::entities::translation(options.document, options.document != Document::Xml1));
    let double_encode = args::bool(arguments, 3).unwrap_or(true);
    let mut result = buffer(string.len().saturating_mul(9))?;
    let mut cursor = 0;
    while cursor < string.len() {
        let rest = &string[cursor..];
        if !double_encode
            && let Some(entity) = entity_at(rest)
            && is_valid_entity(entity, options)
        {
            let length = entity.len() + 2;
            result.extend_from_slice(&rest[..length]);
            cursor += length;
            continue;
        }
        match encoding.next(rest) {
            Decoded::Invalid { length } => {
                match options.invalid {
                    InvalidInput::Ignore => {}
                    InvalidInput::Substitute => result.extend_from_slice(encoding.replacement()),
                    InvalidInput::Reject => return Ok(Value::from("")),
                }
                cursor += length;
            }
            Decoded::Valid { code, length } => {
                if let Some((name, extra)) = table
                    .and_then(|rows| full::entry(rows, &encoding, options, code, &rest[length..]))
                {
                    result.push(b'&');
                    result.extend_from_slice(name.as_bytes());
                    result.push(b';');
                    cursor += extra;
                } else if let Some(escaped) = escape(rest[0], options) {
                    result.extend_from_slice(escaped);
                } else if options.replace_disallowed
                    && if full {
                        !options.document.allows(full::unicode(&encoding, code))
                    } else {
                        encoding.disallowed(code, options.document)
                    }
                {
                    result.extend_from_slice(encoding.replacement());
                } else {
                    result.extend_from_slice(&rest[..length]);
                }
                cursor += length;
            }
        }
    }
    Ok(Value::from(result))
}

/// The entity a special character becomes, if it is one. HTML 4.01 has no
/// `&apos;`, so PHP writes the apostrophe as a number there.
fn escape(byte: u8, options: Options) -> Option<&'static [u8]> {
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

/// `htmlspecialchars_decode(string $string, int $flags = ENT_QUOTES |
/// ENT_SUBSTITUTE | ENT_HTML401): string`
///
/// Only the entities `htmlspecialchars` writes are decoded, by name or by
/// number; `&copy;` stays as it is.
///
/// ```php
/// htmlspecialchars_decode("&lt;b&gt; &#39;x&#x27; &copy;"); // "<b> 'x' &copy;"
/// htmlspecialchars_decode("&quot;", ENT_NOQUOTES);          // "&quot;"
/// ```
const HTMLSPECIALCHARS_DECODE: FunctionEntry = FunctionEntry::sync(
    "htmlspecialchars_decode",
    &[
        Param::new("string", ParamType::String),
        Param::new("flags", ParamType::Int).optional(),
    ],
    htmlspecialchars_decode,
);

fn htmlspecialchars_decode(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let string = args::bytes(arguments, 0);
    let options = Options::from_flags(args::int(arguments, 1).unwrap_or(DEFAULT_FLAGS));
    let mut result = buffer(string.len())?;
    let mut cursor = 0;
    while cursor < string.len() {
        let rest = &string[cursor..];
        if let Some(entity) = entity_at(rest)
            && let Some(decoded) = special_character(entity, options.document)
            && options.quotes.includes(decoded)
        {
            result.push(decoded);
            cursor += entity.len() + 2;
            continue;
        }
        result.push(rest[0]);
        cursor += 1;
    }
    Ok(Value::from(result))
}

/// The name of an entity at the start of `rest`, between `&` and `;`.
fn entity_at(rest: &[u8]) -> Option<&[u8]> {
    let name = rest.strip_prefix(b"&")?;
    let end = name.iter().position(|&byte| byte == b';')?;
    Some(&name[..end])
}

/// The code point of a numeric entity name such as `#39` or `#x27`; no
/// sign or space is allowed.
fn numeric_entity(name: &[u8]) -> Option<u32> {
    let digits = name.strip_prefix(b"#")?;
    let (radix, digits) = match digits {
        [b'x' | b'X', rest @ ..] => (16, rest),
        _ => (10, digits),
    };
    parse_numeric_digits(digits, radix)
}

fn parse_numeric_digits(digits: &[u8], radix: u32) -> Option<u32> {
    if digits.is_empty() {
        return None;
    }
    let mut code = 0u32;
    for &byte in digits {
        let digit = u32::from(hex_digit(byte)?);
        if digit >= radix {
            return None;
        }
        code = code.checked_mul(radix)?.checked_add(digit)?;
    }
    (code <= 0x10_ffff).then_some(code)
}

/// The special character an entity stands for, if it is one of the five.
fn special_character(name: &[u8], document: Document) -> Option<u8> {
    let code = match name {
        b"amp" => 38,
        b"lt" => 60,
        b"gt" => 62,
        b"quot" => 34,
        b"apos" if document != Document::Html401 => 39,
        _ => numeric_entity(name)?,
    };
    matches!(code, 34 | 38 | 39 | 60 | 62).then_some(code as u8)
}

/// Whether `htmlspecialchars` without double encoding keeps `&name;` as it
/// is: a named entity the document type knows, or a number it allows.
fn is_valid_entity(name: &[u8], options: Options) -> bool {
    let document = options.document;
    if let Some(code) = numeric_entity(name) {
        return !options.replace_disallowed
            || match document {
                Document::Html401 => true,
                Document::Html5 => {
                    (document.allows(code) || (0xd800..=0xdfff).contains(&code)) && code != 0x0d
                }
                _ => document.allows(code),
            };
    }
    entities::named(name, document).is_some()
}
