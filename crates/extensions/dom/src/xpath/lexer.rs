//! Tokenize XPath syntax without treating quoted operators as markup.
//!
//! ```php
//! $xpath->query("//item[@name='a/b']");
//! ```

use super::Error;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Token<'a> {
    Name(&'a str),
    String(&'a str),
    Number(f64),
    Slash,
    DoubleSlash,
    ColonColon,
    Dot,
    DoubleDot,
    At,
    Star,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Plus,
    Minus,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Pipe,
    End,
}

pub(super) fn tokens(mut source: &str) -> Result<Vec<Token<'_>>, Error> {
    use Token::*;
    let mut output = Vec::new();
    while !source.is_empty() {
        source = source.trim_start_matches([' ', '\n', '\r', '\t']);
        let Some(first) = source.chars().next() else {
            break;
        };
        if matches!(first, '\'' | '"') {
            let (value, remaining) = source[1..].split_once(first).ok_or(Error::Invalid)?;
            output.push(String(value));
            source = remaining;
            continue;
        }
        if first.is_ascii_digit()
            || (first == '.' && source.as_bytes().get(1).is_some_and(u8::is_ascii_digit))
        {
            let digits = source.bytes().take_while(u8::is_ascii_digit).count();
            let length = if source.as_bytes().get(digits) == Some(&b'.') {
                digits
                    + 1
                    + source[digits + 1..]
                        .bytes()
                        .take_while(u8::is_ascii_digit)
                        .count()
            } else {
                digits
            };
            output.push(Number(
                source[..length].parse().map_err(|_| Error::Invalid)?,
            ));
            source = &source[length..];
            continue;
        }
        if name_start(first) {
            let end = name_end(source);
            output.push(Name(&source[..end]));
            source = &source[end..];
            continue;
        }
        let (token, length) = match source.as_bytes() {
            [b'/', b'/', ..] => (DoubleSlash, 2),
            [b':', b':', ..] => (ColonColon, 2),
            [b'.', b'.', ..] => (DoubleDot, 2),
            [b'!', b'=', ..] => (NotEqual, 2),
            [b'<', b'=', ..] => (LessEqual, 2),
            [b'>', b'=', ..] => (GreaterEqual, 2),
            [b'/', ..] => (Slash, 1),
            [b'.', ..] => (Dot, 1),
            [b'@', ..] => (At, 1),
            [b'*', ..] => (Star, 1),
            [b'(', ..] => (LParen, 1),
            [b')', ..] => (RParen, 1),
            [b'[', ..] => (LBracket, 1),
            [b']', ..] => (RBracket, 1),
            [b',', ..] => (Comma, 1),
            [b'+', ..] => (Plus, 1),
            [b'-', ..] => (Minus, 1),
            [b'=', ..] => (Equal, 1),
            [b'<', ..] => (Less, 1),
            [b'>', ..] => (Greater, 1),
            [b'|', ..] => (Pipe, 1),
            _ => return Err(Error::Invalid),
        };
        output.push(token);
        source = &source[length..];
    }
    output.push(End);
    Ok(output)
}

fn name_start(c: char) -> bool {
    c == '_' || (c != ':' && crate::xml::names::valid(c.encode_utf8(&mut [0; 4])))
}

fn name_end(source: &str) -> usize {
    let mut seen_colon = false;
    for (offset, c) in source.char_indices() {
        if c == ':' && !seen_colon && !source[offset..].starts_with("::") {
            seen_colon = true;
        } else if !(name_start(c) || matches!(c, '-' | '.' | '0'..='9' | '\u{b7}')) {
            return if seen_colon && c == '*' && source[..offset].ends_with(':') {
                offset + 1
            } else {
                offset
            };
        }
    }
    source.len()
}
