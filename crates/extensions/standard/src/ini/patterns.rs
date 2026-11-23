//! `parse_ini_string('n=1e3', scanner_mode: INI_SCANNER_TYPED)` leaves `1e3` a string.

use super::scanner::State;
use super::token::Token;

pub(super) fn horizontal(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

pub(super) fn newline(byte: u8) -> bool {
    matches!(byte, b'\r' | b'\n')
}

pub(super) fn newline_width(input: &[u8]) -> usize {
    match input {
        [b'\r', b'\n', ..] => 2,
        [b'\r' | b'\n', ..] => 1,
        _ => 0,
    }
}

pub(super) fn prefix(input: &[u8], accept: impl Fn(u8) -> bool) -> usize {
    input.iter().take_while(|&&byte| accept(byte)).count()
}

pub(super) fn label(byte: u8) -> bool {
    !b"=\n\r\t;&|^$~(){}!\"[]\0".contains(&byte)
}

pub(super) fn trim(input: &[u8]) -> &[u8] {
    let leading = prefix(input, horizontal);
    let trailing = input
        .iter()
        .rposition(|&byte| !horizontal(byte))
        .map_or(leading, |i| i + 1);
    &input[leading..trailing]
}

pub(super) fn keyword(input: &[u8]) -> Option<(usize, Token<'_>)> {
    // The PHP scanner's generated patterns are case insensitive.
    for (word, token) in [
        (&b"true"[..], Token::True),
        (b"on", Token::True),
        (b"yes", Token::True),
        (b"false", Token::False),
        (b"off", Token::False),
        (b"none", Token::False),
        (b"no", Token::False),
        (b"null", Token::Null),
    ] {
        let length = word.len();
        if input
            .get(..length)
            .is_some_and(|bytes| bytes.eq_ignore_ascii_case(word))
        {
            return Some((length + prefix(&input[length..], horizontal), token));
        }
    }
    None
}

pub(super) fn number(input: &[u8]) -> usize {
    let digits = |bytes: &[u8]| prefix(bytes, |byte| byte.is_ascii_digit());
    if let Some(unsigned) = input.strip_prefix(b"-") {
        let length = digits(unsigned);
        return if length > 0 { length + 1 } else { 0 };
    }
    let first = digits(input);
    if input.get(first) == Some(&b'.') {
        let fraction = digits(&input[first + 1..]);
        if first > 0 || fraction > 0 {
            return first + 1 + fraction;
        }
    }
    first
}

pub(super) fn constant(input: &[u8]) -> usize {
    if matches!(input.first(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_')) {
        prefix(input, |byte| byte.is_ascii_alphanumeric() || byte == b'_')
    } else {
        0
    }
}

pub(super) fn text(input: &[u8], state: State) -> usize {
    let mut position = 0;
    while let Some((&byte, rest)) = input[position..].split_first() {
        position += match byte {
            b'$' => match rest {
                [b'\\', _, ..] => 3,
                [next, ..] if *next != b'{' && *next != 0 => 2,
                _ => break,
            },
            b'\\' if state != State::Value => {
                if rest.is_empty() {
                    break;
                }
                2
            }
            _ if text_byte(state, byte) => 1,
            _ => break,
        };
    }
    position
}

fn text_byte(state: State, byte: u8) -> bool {
    let excluded: &[u8] = match state {
        State::Value => b"$= \t\n\r;&|^~()!\"'\0",
        State::VarFallback => b"$\n\r;\"'}\\",
        _ => b"$\n\r;\"']\\",
    };
    !excluded.contains(&byte)
}

pub(super) fn comment(input: &[u8]) -> usize {
    if let Some(body) = input.strip_prefix(b";") {
        let length = prefix(body, |byte| !newline(byte));
        let width = newline_width(&body[length..]);
        if width != 0 {
            return 1 + length + width;
        }
    }
    0
}
