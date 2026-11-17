//! Provide parser input for `TOKEN_PARSE` even for PHP's legacy string syntax.
//!
//! For example, `"${name}"` can be validated as `"{$name}"`. The returned
//! tokens always point to the input, so the edit cannot change token text or positions.

use phera_lexer::TokenKind;

use crate::stream::SourceToken;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    DoubleQuote,
    Backquote,
    Heredoc,
    Expression,
    DynamicVariable,
}

pub(crate) struct ParserInput {
    pub(crate) source: String,
    starts: Vec<usize>,
}

impl ParserInput {
    pub(crate) fn from_tokens(tokens: &[SourceToken<'_>]) -> Self {
        use TokenKind::*;

        let mut source = std::string::String::new();
        let mut starts = Vec::with_capacity(tokens.len());
        let mut modes = Vec::new();
        let mut halt = false;
        for (index, token) in tokens.iter().enumerate() {
            starts.push(source.len());
            let top = modes.last().copied();
            match token.kind {
                OpenTag if token.text == b"<?" => source.push_str("<?php "),
                // PHP parses parenthesized exit/die with the regular call grammar.
                Exit if next_kind(&tokens[index + 1..]) == Some(LeftParen) => {
                    source.push_str("__phera_tokenizer_exit");
                }
                HaltCompiler => {
                    halt = true;
                    append(&mut source, token.text);
                }
                CloseTag if halt && is_halt_end(&tokens[..index]) => source.push(';'),
                Backquote | DoubleQuote => {
                    let delimiter = if token.kind == Backquote {
                        Mode::Backquote
                    } else {
                        Mode::DoubleQuote
                    };
                    if top == Some(delimiter) {
                        modes.pop();
                    } else {
                        modes.push(delimiter);
                    }
                    if token.kind == Backquote {
                        source.push('"');
                    } else {
                        append(&mut source, token.text);
                    }
                }
                StartHeredoc => {
                    modes.push(Mode::Heredoc);
                    append(&mut source, token.text);
                }
                EndHeredoc => {
                    modes.pop();
                    append(&mut source, token.text);
                }
                DollarCurlyOpen => {
                    if tokens
                        .get(index + 1)
                        .is_some_and(|token| token.kind == StringVarname)
                    {
                        modes.push(Mode::Expression);
                        source.push_str("{$");
                    } else {
                        modes.push(Mode::DynamicVariable);
                        source.push_str("{${");
                    }
                }
                CurlyOpen | LeftBrace => {
                    modes.push(Mode::Expression);
                    append(&mut source, token.text);
                }
                RightBrace => {
                    if modes.pop() == Some(Mode::DynamicVariable) {
                        source.push('}');
                    }
                    append(&mut source, token.text);
                }
                StringPart if top == Some(Mode::Backquote) => {
                    for character in std::string::String::from_utf8_lossy(token.text).chars() {
                        if character == '"' {
                            source.push('\\');
                        }
                        source.push(character);
                    }
                }
                _ => append(&mut source, token.text),
            }
        }
        Self { source, starts }
    }

    pub(crate) fn token_at(&self, offset: usize) -> Option<usize> {
        if offset >= self.source.len() {
            return None;
        }
        let index = self.starts.partition_point(|&start| start < offset);
        if self.starts.get(index) == Some(&offset) {
            return Some(index);
        }
        index.checked_sub(1)
    }
}

fn append(source: &mut String, bytes: &[u8]) {
    source.push_str(&String::from_utf8_lossy(bytes));
}

fn next_kind(tokens: &[SourceToken<'_>]) -> Option<TokenKind> {
    tokens
        .iter()
        .find(|token| !crate::token::ignorable(token.id))
        .map(|token| token.kind)
}

fn is_halt_end(tokens: &[SourceToken<'_>]) -> bool {
    use TokenKind::*;
    tokens
        .iter()
        .rev()
        .filter(|token| !crate::token::ignorable(token.id))
        .take(3)
        .map(|token| token.kind)
        .eq([RightParen, LeftParen, HaltCompiler])
}
