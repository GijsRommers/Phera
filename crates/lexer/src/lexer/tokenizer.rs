//! Scanner behavior for `token_get_all("<?php /* comment */ echo 42;")`.
//!
//! The tokenizer exposes trivia and keeps scanning malformed source. The
//! execution lexer skips trivia and reports syntax it cannot pass to the parser.

use super::{Lexer, Mode, Token, TokenKind, find_bytes, is_whitespace};

impl<'src> Lexer<'src> {
    /// Produces PHP's raw tokenizer stream, including trivia and bad characters.
    pub fn for_tokenizer(source: &'src [u8]) -> Self {
        Self {
            source,
            position: 0,
            modes: vec![Mode::Html],
            halt_countdown: None,
            previous: TokenKind::Eof,
            tokenizer: true,
        }
    }

    pub(super) fn trivia_at(&self, start: usize) -> Option<(TokenKind, usize, bool)> {
        let rest = &self.source[start..];
        let (kind, length, complete) = match rest {
            [first, ..] if is_whitespace(char::from(*first)) => {
                let length = rest
                    .iter()
                    .take_while(|&&b| is_whitespace(char::from(b)))
                    .count();
                (TokenKind::Whitespace, length, true)
            }
            [b'/', b'/', ..] | [b'#', ..] if !rest.starts_with(b"#[") => {
                let length = rest
                    .iter()
                    .enumerate()
                    .find(|&(i, b)| matches!(b, b'\r' | b'\n') || rest[i..].starts_with(b"?>"))
                    .map_or(rest.len(), |(i, _)| i);
                (TokenKind::Comment, length, true)
            }
            [b'/', b'*', body @ ..] => {
                let kind = match body {
                    [b'*', next, ..] if is_whitespace(char::from(*next)) => TokenKind::DocComment,
                    _ => TokenKind::Comment,
                };
                match find_bytes(body, b"*/") {
                    Some(index) => (kind, index + 4, true),
                    None => (kind, rest.len(), false),
                }
            }
            _ => return None,
        };
        Some((kind, start + length, complete))
    }

    pub(super) fn lex_varname(&mut self) -> Result<Token, crate::LexError> {
        let start = self.position;
        let name = self.name_at(start);
        self.begin(Mode::Php);
        if !name.is_empty() && matches!(self.source.get(start + name.len()), Some(b'[' | b'}')) {
            self.position += name.len();
            Ok(self.token(TokenKind::StringVarname, start))
        } else {
            self.lex_php_counting_halt()
        }
    }

    pub(super) fn integer_kind(&self, start: usize) -> TokenKind {
        if !self.tokenizer {
            return TokenKind::Integer;
        }
        let (digits, radix) = match &self.source[start..self.position] {
            [b'0', b'x' | b'X', digits @ ..] => (digits, 16),
            [b'0', b'b' | b'B', digits @ ..] => (digits, 2),
            [b'0', b'o' | b'O', digits @ ..] => (digits, 8),
            [b'0', digits @ ..] => (digits, 8),
            digits => (digits, 10),
        };
        let mut value = Some(0i64);
        for byte in digits.iter().copied().filter(|&b| b != b'_') {
            let Some(digit) = char::from(byte).to_digit(radix) else {
                break;
            };
            value = value
                .and_then(|value| value.checked_mul(i64::from(radix)))
                .and_then(|value| value.checked_add(i64::from(digit)));
        }
        if value.is_some() {
            TokenKind::Integer
        } else {
            TokenKind::Float
        }
    }

    pub(super) fn ampersand_kind(&self) -> TokenKind {
        let after = self.trivia_end(self.position).unwrap_or(self.position);
        let rest = &self.source[after..];
        if rest.starts_with(b"$") || rest.starts_with(b"...") {
            TokenKind::AmpersandBeforeVariable
        } else {
            TokenKind::Ampersand
        }
    }
}
