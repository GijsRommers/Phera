//! `parse_ini_string("a = \\"text\\"; comment\\n")` follows Zend's scanner states.
//!
//! The rules and longest-match ordering come from PHP 8.5.12's
//! Zend/zend_ini_scanner.l. Quoted strings and variable fallbacks can nest,
//! so the scanner uses a stack of conditions.

use std::borrow::Cow;

use super::{Mode, patterns::*, token::Token};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum State {
    Initial,
    Value,
    Raw,
    Section,
    SectionRaw,
    Offset,
    DoubleQuotes,
    VarName,
    VarFallback,
}

pub(super) struct Scanner<'a> {
    input: &'a [u8],
    pub(super) line: usize,
    state: State,
    parents: Vec<State>,
    mode: Mode,
}

impl<'a> Scanner<'a> {
    pub(super) fn new(input: &'a [u8], mode: Mode) -> Self {
        let input = if input.len() > 3 {
            input.strip_prefix(b"\xef\xbb\xbf").unwrap_or(input)
        } else {
            input
        };
        Self {
            input,
            line: 1,
            state: State::Initial,
            parents: Vec::new(),
            mode,
        }
    }

    fn consume(&mut self, length: usize) -> &'a [u8] {
        let (consumed, rest) = self.input.split_at(length);
        self.input = rest;
        consumed
    }

    fn enter(&mut self, state: State, length: usize) {
        self.consume(length);
        self.parents.push(self.state);
        self.state = state;
    }

    fn leave(&mut self, length: usize) {
        self.consume(length);
        self.state = self.parents.pop().unwrap_or(State::Initial);
    }

    pub(super) fn next(&mut self) -> Token<'a> {
        loop {
            if self.input.is_empty() {
                if matches!(self.state, State::Value | State::Raw) {
                    self.state = State::Initial;
                    return Token::EndLine;
                }
                return Token::End;
            }
            match self.state {
                State::Initial => {
                    if let Some(token) = self.initial() {
                        return token;
                    }
                }
                State::Raw => {
                    if let Some(token) = self.raw() {
                        return token;
                    }
                }
                State::SectionRaw => return self.raw_section(),
                State::DoubleQuotes => return self.quoted(),
                State::VarName => return self.variable_name(),
                _ => return self.value(),
            }
        }
    }

    fn initial(&mut self) -> Option<Token<'a>> {
        let input = self.input;
        if input.starts_with(b"[") {
            self.consume(1);
            self.state = if self.mode == Mode::Raw {
                State::SectionRaw
            } else {
                State::Section
            };
            return Some(Token::Section);
        }
        let space = prefix(input, horizontal);
        let after_space = &input[space..];
        let end = newline_width(after_space).max(comment(after_space));
        if end > 0 {
            return Some(self.end_line(space + end));
        }
        if let Some(value) = after_space.strip_prefix(b"=") {
            self.consume(space + 1 + prefix(value, horizontal));
            self.state = if self.mode == Mode::Raw {
                State::Raw
            } else {
                State::Value
            };
            return Some(Token::Punctuation(b'='));
        }
        let length = prefix(input, label);
        if length > 0 {
            if let Some(offset) = input[length..].strip_prefix(b"[") {
                self.consume(length + 1 + prefix(offset, horizontal));
                self.state = State::Offset;
                return Some(Token::Offset(trim(&input[..length])));
            }
            if let Some((size, kind)) = keyword(input).filter(|(size, _)| *size >= length) {
                self.consume(size);
                return Some(kind);
            }
            return Some(Token::Label(trim(self.consume(length))));
        }
        if space > 0 {
            self.consume(space);
            return None;
        }
        let byte = input[0];
        Some(if b":,.[]\"'()&|^+-/*=%$!~<>?@{}".contains(&byte) {
            self.consume(1);
            Token::Punctuation(byte)
        } else {
            Token::End
        })
    }

    fn raw(&mut self) -> Option<Token<'a>> {
        let input = self.input;
        let space = prefix(input, horizontal);
        let end = &input[space..];
        let length = newline_width(end).max(comment(end));
        if length > 0 {
            return Some(self.end_line(space + length));
        }
        if matches!(input[0], b';' | 0) {
            self.consume(1);
            self.state = State::Initial;
            return Some(Token::EndLine);
        }
        let length = prefix(input, |byte| !newline(byte));
        let line = self.consume(length);
        let content = trim(line);
        let quoted = content.first() == Some(&b'"');
        let mut comment = None;
        for (index, &byte) in content.iter().enumerate() {
            match byte {
                b';' if comment.is_none() => comment = Some(index),
                b'"' if quoted => comment = None,
                _ => {}
            }
        }
        let content = trim(&content[..comment.unwrap_or(content.len())]);
        let unquoted = content
            .strip_prefix(b"\"")
            .and_then(|text| text.strip_suffix(b"\""));
        Some(Token::Raw(unquoted.unwrap_or(content)))
    }

    fn raw_section(&mut self) -> Token<'a> {
        if self.input.starts_with(b"]") {
            return self.end_section();
        }
        let length = prefix(self.input, |byte| byte != b']' && !newline(byte));
        if length == 0 {
            Token::End
        } else {
            Token::Raw(self.consume(length))
        }
    }

    fn end_section(&mut self) -> Token<'a> {
        let rest = &self.input[1..];
        let spaces = prefix(rest, horizontal);
        self.consume(1 + spaces + newline_width(&rest[spaces..]));
        self.state = State::Initial;
        self.line += 1;
        Token::Punctuation(b']')
    }

    fn end_line(&mut self, length: usize) -> Token<'a> {
        self.consume(length);
        self.state = State::Initial;
        self.line += 1;
        Token::EndLine
    }

    fn value(&mut self) -> Token<'a> {
        let input = self.input;
        let spaces = prefix(input, horizontal);
        let head = &input[spaces..];
        match self.state {
            State::Section if input.starts_with(b"]") => return self.end_section(),
            State::Offset if head.starts_with(b"]") => {
                self.consume(spaces + 1);
                self.state = State::Initial;
                return Token::Punctuation(b']');
            }
            State::VarFallback if input.starts_with(b"}") => {
                self.leave(1);
                return Token::Punctuation(b'}');
            }
            State::Value => {
                let length = newline_width(head).max(comment(head));
                if length > 0 {
                    return self.end_line(spaces + length);
                }
                match input[0] {
                    b'=' => {
                        self.state = State::Initial;
                        return Token::EndLine;
                    }
                    byte if b"&|^~()!".contains(&byte) => {
                        self.consume(1 + prefix(&input[1..], horizontal));
                        return Token::Punctuation(byte);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        if head.starts_with(b"\"") {
            self.enter(State::DoubleQuotes, spaces + 1);
            return Token::Punctuation(b'"');
        }
        if input.starts_with(b"${") {
            self.enter(State::VarName, 2);
            return Token::Variable;
        }
        if self.state != State::VarFallback
            && let Some(rest) = input.strip_prefix(b"'")
            && let Some(end) = rest
                .iter()
                .position(|byte| *byte == b'\'')
                .filter(|i| *i > 0)
        {
            self.consume(end + 2);
            return Token::Raw(&rest[..end]);
        }
        let text = text(input, self.state);
        let constant = constant(input);
        let number = number(input);
        let longest = text.max(constant).max(number).max(spaces);
        if self.state == State::Value
            && let Some((length, token)) = keyword(input).filter(|(size, _)| *size >= longest)
        {
            self.consume(length);
            return token;
        }
        if longest == 0 {
            if self.state == State::Value {
                self.consume(1);
                self.state = State::Initial;
                return Token::EndLine;
            }
            return Token::End;
        }
        let bytes = self.consume(longest);
        if constant == longest {
            Token::Constant(bytes)
        } else if number == longest {
            Token::Number(bytes)
        } else if text == longest {
            Token::Text(bytes)
        } else {
            Token::Space(bytes)
        }
    }

    fn variable_name(&mut self) -> Token<'a> {
        let input = self.input;
        if input.starts_with(b"}") {
            self.leave(1);
            return Token::Punctuation(b'}');
        }
        if input.starts_with(b":-") {
            self.consume(2);
            self.state = State::VarFallback;
            return Token::Fallback;
        }
        let mut length = 0;
        while let Some(byte) = input.get(length).copied() {
            if !label(byte) || input[length..].starts_with(b":-") {
                break;
            }
            length += 1;
        }
        if length == 0 {
            Token::End
        } else {
            Token::VarName(trim(self.consume(length)))
        }
    }

    fn quoted(&mut self) -> Token<'a> {
        let input = self.input;
        if input.starts_with(b"\"") {
            self.leave(1 + prefix(&input[1..], horizontal));
            return Token::Punctuation(b'"');
        }
        if input.starts_with(b"${") {
            self.enter(State::VarName, 2);
            return Token::Variable;
        }
        let mut width = 0;
        while let Some(&byte) = input.get(width) {
            match byte {
                b'"' => break,
                b'$' if input[width..].starts_with(b"${") => break,
                b'\\' if let Some(&escaped) = input.get(width + 1) => {
                    if escaped == b'"' && input.get(width + 2).is_none_or(|&next| newline(next)) {
                        width += 1;
                        break;
                    }
                    width += 2;
                }
                _ => width += 1,
            }
        }
        let bytes = self.consume(width);
        self.line += bytes
            .iter()
            .enumerate()
            .filter(|&(i, &byte)| {
                byte == b'\n' || (byte == b'\r' && input.get(i + 1) != Some(&b'\n'))
            })
            .count();
        Token::Quoted(unescape(bytes))
    }
}

fn unescape(bytes: &[u8]) -> Cow<'_, [u8]> {
    if !bytes.contains(&b'\\') {
        return Cow::Borrowed(bytes);
    }
    let mut output = Vec::with_capacity(bytes.len());
    let mut input = bytes.iter().copied().peekable();
    while let Some(byte) = input.next() {
        if byte == b'\\' && matches!(input.peek(), Some(b'"' | b'\\' | b'$')) {
            output.extend(input.next());
        } else {
            output.push(byte);
        }
    }
    Cow::Owned(output)
}
