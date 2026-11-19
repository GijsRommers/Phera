//! In `parse_ini_string('name="${USER}"')`, a double-quoted value can interpolate variables.

use std::borrow::Cow;

use super::{Scanner, State, Token, horizontal, label, newline, prefix, trim};

impl<'a> Scanner<'a> {
    pub(super) fn variable_name(&mut self) -> Token<'a> {
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
        if length == 1 && input[length..].starts_with(b":-") {
            // Zend tests the fallback marker immediately after the first name byte.
            self.consume(2);
            self.state = State::VarFallback;
            return Token::Fallback;
        }
        if length == 0 {
            Token::End
        } else {
            Token::VarName(trim(self.consume(length)))
        }
    }

    pub(super) fn quoted(&mut self) -> Token<'a> {
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
