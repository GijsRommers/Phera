use phera_source::Span;

use crate::{
    LexError,
    LexErrorKind,
    Token,
    TokenKind,
};

pub struct Lexer<'src> {
    source: &'src str,
    position: usize,
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            position: 0,
        }
    }

    pub fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace();

        let start = self.position;

        if self.is_eof() {
            return Ok(Token {
                kind: TokenKind::Eof,
                span: Span::new(start, start),
            });
        }

        if self.starts_with("<?php") {
            self.position += 5;

            return Ok(Token {
                kind: TokenKind::OpenTag,
                span: Span::new(start, self.position),
            });
        }

        if self.starts_with("?>") {
            self.position += 2;

            return Ok(Token {
                kind: TokenKind::CloseTag,
                span: Span::new(start, self.position),
            });
        }

        let c = self.advance().unwrap();

        let kind = match c {
            '$' => self.lex_variable(),

            '0'..='9' => self.lex_number(),

            'a'..='z' | 'A'..='Z' | '_' => {
                self.lex_identifier(start)
            }

            '\'' | '"' => self.lex_string(c)?,

            '+' => TokenKind::Plus,

            '-' => {
                if self.consume_if('>') {
                    TokenKind::Arrow
                } else {
                    TokenKind::Minus
                }
            }

            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '.' => TokenKind::Dot,

            '=' => {
                if self.consume_if('=') {
                    if self.consume_if('=') {
                        TokenKind::Identical
                    } else {
                        TokenKind::Equal
                    }
                } else if self.consume_if('>') {
                    TokenKind::FatArrow
                } else {
                    TokenKind::Assign
                }
            }

            '!' => {
                if self.consume_if('=') {
                    if self.consume_if('=') {
                        TokenKind::NotIdentical
                    } else {
                        TokenKind::NotEqual
                    }
                } else {
                    return self.unexpected(start, c);
                }
            }

            '<' => {
                if self.consume_if('=') {
                    TokenKind::LessEqual
                } else {
                    TokenKind::Less
                }
            }

            '>' => {
                if self.consume_if('=') {
                    TokenKind::GreaterEqual
                } else {
                    TokenKind::Greater
                }
            }

            '?' => {
                if self.consume_if('?') {
                    TokenKind::NullCoalesce
                } else {
                    return self.unexpected(start, c);
                }
            }

            ':' => {
                if self.consume_if(':') {
                    TokenKind::DoubleColon
                } else {
                    TokenKind::Colon
                }
            }

            '(' => TokenKind::LeftParen,
            ')' => TokenKind::RightParen,
            '{' => TokenKind::LeftBrace,
            '}' => TokenKind::RightBrace,
            '[' => TokenKind::LeftBracket,
            ']' => TokenKind::RightBracket,

            ';' => TokenKind::Semicolon,
            ',' => TokenKind::Comma,

            _ => return self.unexpected(start, c),
        };

        Ok(Token {
            kind,
            span: Span::new(start, self.position),
        })
    }

    fn lex_identifier(&mut self, start: usize) -> TokenKind {
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }

        match &self.source[start..self.position] {
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "function" => TokenKind::Function,
            "return" => TokenKind::Return,
            "echo" => TokenKind::Echo,
            "class" => TokenKind::Class,
            "new" => TokenKind::New,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "null" => TokenKind::Null,
            _ => TokenKind::Identifier,
        }
    }

    fn lex_variable(&mut self) -> TokenKind {
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }

        TokenKind::Variable
    }

    fn lex_number(&mut self) -> TokenKind {
        while matches!(self.peek(), Some('0'..='9')) {
            self.advance();
        }

        if self.peek() == Some('.')
            && matches!(self.peek_next(), Some('0'..='9'))
        {
            self.advance();

            while matches!(self.peek(), Some('0'..='9')) {
                self.advance();
            }

            TokenKind::Float
        } else {
            TokenKind::Integer
        }
    }

    fn lex_string(&mut self, quote: char) -> Result<TokenKind, LexError> {
        let start = self.position - quote.len_utf8();

        while let Some(c) = self.peek() {
            if c == quote {
                self.advance();
                return Ok(TokenKind::String);
            }

            if c == '\\' {
                self.advance();

                if !self.is_eof() {
                    self.advance();
                }

                continue;
            }

            self.advance();
        }

        Err(LexError {
            kind: LexErrorKind::UnterminatedString,
            span: Span::new(start, self.position),
        })
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.advance();
        }
    }

    fn unexpected<T>(
        &self,
        start: usize,
        c: char,
    ) -> Result<T, LexError> {
        Err(LexError {
            kind: LexErrorKind::UnexpectedCharacter(c),
            span: Span::new(start, self.position),
        })
    }

    fn starts_with(&self, value: &str) -> bool {
        self.source[self.position..].starts_with(value)
    }

    fn consume_if(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        let mut chars = self.source[self.position..].chars();

        chars.next()?;
        chars.next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.position += c.len_utf8();
        Some(c)
    }

    fn is_eof(&self) -> bool {
        self.position >= self.source.len()
    }
}