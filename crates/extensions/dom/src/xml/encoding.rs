//! Decode the supported XML encodings before handing input to quick-xml.
//!
//! ```php
//! $document->loadXML('<?xml version="1.0" encoding="ISO-8859-1"?><r>'."\xe9".'</r>');
//! ```

use std::borrow::Cow;

use quick_xml::Reader;
use quick_xml::events::Event;

use crate::document::ParseError;

pub fn supported_encoding(name: &str) -> bool {
    matches!(
        name.to_ascii_uppercase().as_str(),
        "UTF-8"
            | "UTF8"
            | "UTF-16"
            | "UTF-16LE"
            | "UTF-16BE"
            | "US-ASCII"
            | "ASCII"
            | "ISO-8859-1"
            | "ISO8859-1"
            | "LATIN1"
    )
}

pub(super) fn decode<'a>(
    input: &'a [u8],
    encoding: Option<&str>,
) -> Result<Cow<'a, str>, ParseError> {
    let encoding = encoding.map(str::to_ascii_uppercase);
    let utf16 = match input {
        [0xff, 0xfe, rest @ ..] => Some((rest, true)),
        [0xfe, 0xff, rest @ ..] => Some((rest, false)),
        [b'<', 0, ..] => Some((input, true)),
        [0, b'<', ..] => Some((input, false)),
        _ => match encoding.as_deref() {
            Some("UTF-16" | "UTF-16LE") => Some((input, true)),
            Some("UTF-16BE") => Some((input, false)),
            _ => None,
        },
    };
    if let Some((bytes, little_endian)) = utf16 {
        if !bytes.len().is_multiple_of(2) {
            return Err(invalid_encoding());
        }
        let units = bytes.as_chunks::<2>().0.iter().map(|&pair| {
            let bytes = pair;
            if little_endian {
                u16::from_le_bytes(bytes)
            } else {
                u16::from_be_bytes(bytes)
            }
        });
        return char::decode_utf16(units)
            .collect::<Result<String, _>>()
            .map(Cow::Owned)
            .map_err(|_| invalid_encoding());
    }

    let bytes = input.strip_prefix(b"\xef\xbb\xbf").unwrap_or(input);
    let encoding = encoding.or_else(|| match Reader::from_reader(bytes).read_event() {
        Ok(Event::Decl(declaration)) => declaration
            .encoding()
            .and_then(Result::ok)
            .map(|name| name.to_ascii_uppercase()),
        _ => None,
    });
    match encoding.as_deref() {
        Some("ISO-8859-1" | "ISO8859-1" | "LATIN1") => {
            if bytes.is_ascii() {
                std::str::from_utf8(bytes)
                    .map(Cow::Borrowed)
                    .map_err(|_| invalid_encoding())
            } else {
                Ok(Cow::Owned(bytes.iter().copied().map(char::from).collect()))
            }
        }
        Some("US-ASCII" | "ASCII") if !bytes.is_ascii() => Err(invalid_encoding()),
        None | Some("UTF-8" | "UTF8" | "US-ASCII" | "ASCII") => std::str::from_utf8(bytes)
            .map(Cow::Borrowed)
            .map_err(|_| invalid_encoding()),
        Some(encoding) => Err(ParseError::fatal(
            32,
            format!("Unsupported encoding {}", encoding),
        )),
    }
}

fn invalid_encoding() -> ParseError {
    ParseError::fatal(9, "Input is not proper UTF-8, indicate encoding !")
}

pub(super) fn xml_char(character: char) -> bool {
    matches!(
        character,
        '\t' | '\r' | '\n' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}'
    )
}
