//! `parse_ini_string("[db]\nport = 3306", true)` returns a nested section array.

use std::rc::Rc;

use phera_extension_api::{ArrayKey, Context, PhpArray, Value};

use super::{Mode, scanner::Scanner, token::Token};

pub(super) struct Parser<'a> {
    scanner: Scanner<'a>,
    pub(super) mode: Mode,
    pub(super) lookahead: Token<'a>,
}

impl<'a> Parser<'a> {
    pub(super) fn new(input: &'a [u8], mode: Mode) -> Self {
        let mut scanner = Scanner::new(input, mode);
        let lookahead = scanner.next();
        Self {
            scanner,
            lookahead,
            mode,
        }
    }

    pub(super) fn take(&mut self) -> Token<'a> {
        std::mem::replace(&mut self.lookahead, self.scanner.next())
    }

    pub(super) fn line(&self) -> usize {
        self.scanner.line
    }

    pub(super) fn syntax(&self, expected: &str) -> String {
        format!(
            "syntax error, unexpected {}{expected}",
            self.lookahead.description()
        )
    }

    pub(super) fn parse(
        &mut self,
        context: &mut dyn Context,
        sections: bool,
    ) -> Result<PhpArray, String> {
        let mut result = PhpArray::new();
        let mut section: Option<(ArrayKey, PhpArray)> = None;
        loop {
            match self.lookahead {
                Token::End => break,
                Token::EndLine => {
                    self.take();
                }
                Token::Section => {
                    self.take();
                    let name = self
                        .fragments(context, true)?
                        .map(|fragment| fragment.bytes)
                        .unwrap_or_default();
                    self.punctuation(b']')?;
                    if sections {
                        if let Some((key, data)) = section.take() {
                            result.insert(key, Value::from(data));
                        }
                        section = Some((key(name), PhpArray::new()));
                    }
                }
                Token::Label(name) | Token::Offset(name) => {
                    let offset = matches!(self.lookahead, Token::Offset(_));
                    self.take();
                    let index = if offset {
                        let fragment = self
                            .fragments(context, false)?
                            .map(|fragment| fragment.bytes)
                            .unwrap_or_default();
                        self.punctuation(b']')?;
                        Some(fragment)
                    } else {
                        None
                    };
                    if offset || matches!(self.lookahead, Token::Punctuation(b'=')) {
                        self.punctuation(b'=')?;
                        let value = self.entry(context)?;
                        let table = section.as_mut().map_or(&mut result, |(_, data)| data);
                        insert(table, name, index, value);
                    }
                }
                _ => return Err(self.syntax("")),
            }
        }
        if let Some((key, data)) = section {
            result.insert(key, Value::from(data));
        }
        Ok(result)
    }

    fn punctuation(&mut self, expected: u8) -> Result<(), String> {
        if matches!(self.lookahead, Token::Punctuation(byte) if byte == expected) {
            self.take();
            Ok(())
        } else {
            Err(self.syntax(&format!(", expecting '{}'", char::from(expected))))
        }
    }

    fn entry(&mut self, context: &mut dyn Context) -> Result<Value, String> {
        let value = match &self.lookahead {
            Token::True if self.mode == Mode::Typed => Value::Bool(true),
            Token::False if self.mode == Mode::Typed => Value::Bool(false),
            Token::Null if self.mode == Mode::Typed => Value::Null,
            Token::True => Value::from("1"),
            Token::False | Token::Null | Token::EndLine => Value::from(""),
            _ => return self.expression(context),
        };
        self.take();
        Ok(value)
    }
}

fn insert(table: &mut PhpArray, name: &[u8], offset: Option<Vec<u8>>, value: Value) {
    let array_key = if offset.is_some() {
        pop_key(name)
    } else {
        key(name.to_vec())
    };
    let Some(offset) = offset else {
        table.insert(array_key, value);
        return;
    };
    if !matches!(table.get(&array_key), Some(Value::Array(_))) {
        table.insert(array_key.clone(), Value::from(PhpArray::new()));
    }
    if let Some(Value::Array(items)) = table.get_mut(&array_key) {
        let items = Rc::make_mut(items);
        if offset.is_empty() {
            let _ = items.push(value);
        } else {
            items.insert(key(offset), value);
        }
    }
}

fn key(name: Vec<u8>) -> ArrayKey {
    let name = Value::from(name);
    ArrayKey::from_value(&name).unwrap_or_else(|_| ArrayKey::str(""))
}

/// Zend uses a base-0 unsigned conversion specifically for the key of x[]=v.
fn pop_key(name: &[u8]) -> ArrayKey {
    let text = std::str::from_utf8(name).ok();
    if name.len() > 1 && name.starts_with(b"0") {
        return ArrayKey::str(name);
    }
    if let Some(text) = text.filter(|text| text.parse::<i64>().is_ok()) {
        let (sign, magnitude) = match text.as_bytes() {
            [b'-', rest @ ..] => (-1i64, rest),
            [b'+', rest @ ..] => (1i64, rest),
            rest => (1i64, rest),
        };
        let radix = if magnitude.starts_with(b"0") { 8 } else { 10 };
        let number = magnitude
            .iter()
            .map(|&byte| u64::from(byte - b'0'))
            .take_while(|&digit| digit < radix)
            .fold(0u64, |value, digit| value * radix + digit);
        return ArrayKey::Int((number as i64).wrapping_mul(sign));
    }
    ArrayKey::str(name)
}
