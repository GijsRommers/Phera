//! Escape character data and attribute values according to the output mode.
//!
//! ```php
//! $document->loadXML('<r a="&quot;">a &amp; b</r>');
//! echo $document->saveXML();
//! ```

use std::fmt::Write;

#[derive(Clone, Copy)]
pub(super) enum Escaping {
    Xml { attribute: bool, ascii: bool },
    Html { attribute: bool, ascii: bool },
    Html5 { attribute: bool },
    Canonical { attribute: bool },
}

pub(super) fn escape(output: &mut String, input: &str, mode: Escaping) {
    let attribute = match mode {
        Escaping::Xml { attribute, .. }
        | Escaping::Html { attribute, .. }
        | Escaping::Html5 { attribute }
        | Escaping::Canonical { attribute } => attribute,
    };
    let prefix = input
        .bytes()
        .position(|b| {
            matches!(
                b,
                b'&' | b'<' | b'>' | b'\r' | b'\n' | b'\t' | b'"' | 128..=255
            )
        })
        .unwrap_or(input.len());
    let (plain, escaped) = input.split_at(prefix);
    output.push_str(plain);
    for character in escaped.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' if !attribute || !matches!(mode, Escaping::Canonical { .. }) => {
                output.push_str("&gt;")
            }
            '\u{a0}' if matches!(mode, Escaping::Html5 { .. }) => output.push_str("&nbsp;"),
            '"' if attribute => output.push_str("&quot;"),
            '\t' | '\n' | '\r' if attribute && matches!(mode, Escaping::Canonical { .. }) => {
                let _ = write!(output, "&#x{:X};", character as u32);
            }
            '\r' if matches!(mode, Escaping::Canonical { .. }) => output.push_str("&#xD;"),
            '\t' | '\n' | '\r' if attribute && matches!(mode, Escaping::Xml { .. }) => {
                let _ = write!(output, "&#{};", character as u32);
            }
            '\r' if matches!(mode, Escaping::Xml { .. }) => output.push_str("&#13;"),
            c if c.is_ascii() => output.push(c),
            c if matches!(mode, Escaping::Xml { ascii: true, .. }) => {
                let _ = write!(output, "&#x{:X};", c as u32);
            }
            c if matches!(mode, Escaping::Html { ascii: true, .. }) => {
                if let Some(entity) = latin_entity(c) {
                    let _ = write!(output, "&{entity};");
                } else {
                    let _ = write!(output, "&#{};", c as u32);
                }
            }
            c => output.push(c),
        }
    }
}

fn latin_entity(character: char) -> Option<&'static str> {
    const LATIN1: [&str; 96] = [
        "nbsp", "iexcl", "cent", "pound", "curren", "yen", "brvbar", "sect", "uml", "copy", "ordf",
        "laquo", "not", "shy", "reg", "macr", "deg", "plusmn", "sup2", "sup3", "acute", "micro",
        "para", "middot", "cedil", "sup1", "ordm", "raquo", "frac14", "frac12", "frac34", "iquest",
        "Agrave", "Aacute", "Acirc", "Atilde", "Auml", "Aring", "AElig", "Ccedil", "Egrave",
        "Eacute", "Ecirc", "Euml", "Igrave", "Iacute", "Icirc", "Iuml", "ETH", "Ntilde", "Ograve",
        "Oacute", "Ocirc", "Otilde", "Ouml", "times", "Oslash", "Ugrave", "Uacute", "Ucirc",
        "Uuml", "Yacute", "THORN", "szlig", "agrave", "aacute", "acirc", "atilde", "auml", "aring",
        "aelig", "ccedil", "egrave", "eacute", "ecirc", "euml", "igrave", "iacute", "icirc",
        "iuml", "eth", "ntilde", "ograve", "oacute", "ocirc", "otilde", "ouml", "divide", "oslash",
        "ugrave", "uacute", "ucirc", "uuml", "yacute", "thorn", "yuml",
    ];
    match character {
        '€' => Some("euro"),
        '–' => Some("ndash"),
        '—' => Some("mdash"),
        '‘' => Some("lsquo"),
        '’' => Some("rsquo"),
        '“' => Some("ldquo"),
        '”' => Some("rdquo"),
        '…' => Some("hellip"),
        c => LATIN1.get((c as usize).checked_sub(160)?).copied(),
    }
}

pub(super) fn encode_xml(input: String, encoding: &str) -> Vec<u8> {
    match encoding.to_ascii_uppercase().as_str() {
        "ISO-8859-1" | "LATIN1" | "ISO8859-1" => {
            let mut output = Vec::with_capacity(input.len());
            for character in input.chars() {
                if let Ok(byte) = u8::try_from(character as u32) {
                    output.push(byte);
                } else {
                    output.extend_from_slice(format!("&#{};", character as u32).as_bytes());
                }
            }
            output
        }
        "UTF-16" | "UTF-16LE" | "UTF-16BE" => {
            let mut output = Vec::with_capacity(input.len().saturating_mul(2));
            if encoding.eq_ignore_ascii_case("UTF-16") {
                output.extend([0xff, 0xfe]);
            }
            for code in input.encode_utf16() {
                if encoding.eq_ignore_ascii_case("UTF-16BE") {
                    output.extend(code.to_be_bytes());
                } else {
                    output.extend(code.to_le_bytes());
                }
            }
            output
        }
        _ => input.into_bytes(),
    }
}
