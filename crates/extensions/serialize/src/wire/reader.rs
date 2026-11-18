//! `unserialize('s:3:"abc";extra')` consumes ten bytes before reporting extra data.

use std::borrow::Cow;

use super::numbers::{digits_end, float_end, signed, signed_end, unsigned};
use super::{Diagnostic, ObjectFormat, ParseError, Token};

#[derive(Clone)]
pub struct Reader<'a> {
    input: &'a [u8],
    offset: usize,
    diagnostic: Option<Diagnostic>,
}

impl<'a> Reader<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Self {
            input,
            offset: 0,
            diagnostic: None,
        }
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn take_diagnostic(&mut self) -> Option<Diagnostic> {
        self.diagnostic.take()
    }

    pub fn consume(&mut self, prefix: &[u8]) -> bool {
        if self.input[self.offset..].starts_with(prefix) {
            self.offset += prefix.len();
            true
        } else {
            false
        }
    }

    pub fn close(&mut self) -> Result<(), ParseError> {
        if self.input.get(self.offset) != Some(&b'}') {
            return Err(ParseError);
        }
        self.offset += 1;
        Ok(())
    }

    /// `key` uses PHP's key grammar, which disallows references and objects.
    pub fn token(&mut self, key: bool) -> Result<Token<'a>, ParseError> {
        let rest = &self.input[self.offset..];
        match rest {
            [b'N', b';', ..] => {
                self.offset += 2;
                Ok(Token::Null)
            }
            [b'b', b':', boolean @ (b'0' | b'1'), b';', ..] => {
                self.offset += 4;
                Ok(Token::Bool(*boolean == b'1'))
            }
            [b'i', b':', ..] => self.integer(),
            [b'd', b':', ..] => self.float(),
            [kind @ (b's' | b'S'), b':', ..] => self.string(*kind == b'S'),
            [b'a', b':', ..] => self.array(key),
            [kind @ (b'O' | b'C'), b':', ..] if !key => self.object(*kind),
            [b'E', b':', ..] if !key => self.enum_case(),
            [kind @ (b'R' | b'r'), b':', ..] if !key => self.reference(*kind),
            [b'}', ..] => {
                self.diagnostic = Some(Diagnostic::UnexpectedEnd);
                Err(ParseError)
            }
            _ => Err(ParseError),
        }
    }

    fn integer(&mut self) -> Result<Token<'a>, ParseError> {
        let start = self.offset + 2;
        let (end, digits) = signed_end(self.input, start);
        if !digits || self.input.get(end) != Some(&b';') {
            return Err(ParseError);
        }
        self.offset = end + 1;
        Ok(Token::Int(self.signed(start, end)))
    }

    fn signed(&mut self, start: usize, end: usize) -> i64 {
        let (value, overflow) = signed(&self.input[start..end]);
        if overflow {
            self.diagnostic = Some(Diagnostic::NumberOutOfRange);
        }
        value
    }

    fn float(&mut self) -> Result<Token<'a>, ParseError> {
        let start = self.offset + 2;
        let (len, value) = match &self.input[start..] {
            [b'N', b'A', b'N', b';', ..] => (3, f64::NAN),
            [b'I', b'N', b'F', b';', ..] => (3, f64::INFINITY),
            [b'-', b'I', b'N', b'F', b';', ..] => (4, f64::NEG_INFINITY),
            _ => {
                let end = float_end(self.input, start).ok_or(ParseError)?;
                if self.input.get(end) != Some(&b';') {
                    return Err(ParseError);
                }
                let digits =
                    std::str::from_utf8(&self.input[start..end]).map_err(|_| ParseError)?;
                (end - start, digits.parse().map_err(|_| ParseError)?)
            }
        };
        self.offset = start + len + 1;
        Ok(Token::Float(value))
    }

    fn length(&self, delimiters: &[u8]) -> Result<(usize, usize), ParseError> {
        let start = self.offset + 2;
        let end = digits_end(self.input, start);
        if end == start || !self.input[end..].starts_with(delimiters) {
            return Err(ParseError);
        }
        Ok((end + delimiters.len(), unsigned(&self.input[start..end])))
    }

    fn quoted_start(&mut self, allow_empty: bool) -> Result<(usize, usize), ParseError> {
        let (start, length) = self.length(b":\"")?;
        if length > self.input.len() - start || (!allow_empty && length == 0) {
            self.offset += 2;
            return Err(ParseError);
        }
        Ok((start, length))
    }

    fn quoted_end(&mut self, end: usize, terminator: u8) -> Result<(), ParseError> {
        self.offset = end;
        if self.input.get(end) != Some(&b'"') {
            return Err(ParseError);
        }
        self.offset += 1;
        if self.input.get(self.offset) != Some(&terminator) {
            return Err(ParseError);
        }
        self.offset += 1;
        Ok(())
    }

    fn string(&mut self, escaped: bool) -> Result<Token<'a>, ParseError> {
        let (start, length) = self.quoted_start(true)?;
        if escaped {
            let mut data = Vec::new();
            let mut cursor = start;
            for _ in 0..length {
                let byte = *self.input.get(cursor).ok_or(ParseError)?;
                cursor += 1;
                if byte == b'\\' {
                    let hex = self.input.get(cursor..cursor + 2).ok_or(ParseError)?;
                    let high = (hex[0] as char).to_digit(16).ok_or(ParseError)?;
                    let low = (hex[1] as char).to_digit(16).ok_or(ParseError)?;
                    data.push((high * 16 + low) as u8);
                    cursor += 2;
                } else {
                    data.push(byte);
                }
            }
            self.quoted_end(cursor, b';')?;
            self.diagnostic = Some(Diagnostic::EscapedString);
            return Ok(Token::String(Cow::Owned(data)));
        }
        let end = start + length;
        self.quoted_end(end, b';')?;
        Ok(Token::String(Cow::Borrowed(&self.input[start..end])))
    }

    fn array(&mut self, key: bool) -> Result<Token<'a>, ParseError> {
        let start = self.offset + 2;
        let end = digits_end(self.input, start);
        if end == start || !self.input[end..].starts_with(b":{") {
            return Err(ParseError);
        }
        self.offset = end + 2;
        if key {
            return Err(ParseError);
        }
        let count = self.signed(start, end);
        Ok(Token::Array(self.checked_count(count, self.offset)?))
    }

    fn checked_count(&self, count: i64, header_end: usize) -> Result<usize, ParseError> {
        let count = usize::try_from(count).map_err(|_| ParseError)?;
        if count >= 0x8000_0000 || count > (self.input.len() - header_end) / 2 {
            return Err(ParseError);
        }
        Ok(count)
    }

    fn object(&mut self, kind: u8) -> Result<Token<'a>, ParseError> {
        let original = self.offset;
        let (start, len) = self.quoted_start(false)?;
        let end = start + len;
        self.quoted_end(end, b':')?;
        let name = &self.input[start..end];
        if matches!(name.first(), Some(0 | b'\\')) || !super::valid_class(name) {
            self.offset = original;
            return Err(ParseError);
        }
        let format = if kind == b'C' {
            ObjectFormat::Custom
        } else {
            ObjectFormat::Properties
        };
        Ok(Token::Object(name, format))
    }

    /// An object header has been read; PHP resolves its class before these bytes.
    pub fn properties(&mut self) -> Result<usize, ParseError> {
        let colon = self.offset.checked_sub(1).ok_or(ParseError)?;
        let quote = colon.checked_sub(1).ok_or(ParseError)?;
        if self.input.len() - quote <= 2 {
            self.offset = quote;
            self.diagnostic = Some(Diagnostic::BadObjectData);
            return Err(ParseError);
        }
        let (count, end) = self.object_count(colon);
        let count = self.checked_count(count, quote)?;
        self.open_object_body(end)?;
        Ok(count)
    }

    pub fn custom_data(&mut self) -> Result<&'a [u8], ParseError> {
        let colon = self.offset.checked_sub(1).ok_or(ParseError)?;
        let (length, end) = self.object_count(colon);
        if self.input.len() - end < 2 {
            return Err(ParseError);
        }
        self.open_object_body(end)?;
        let available = self.input.len() - self.offset;
        let size = usize::try_from(length)
            .ok()
            .filter(|&size| size < available);
        let Some(size) = size else {
            self.diagnostic = Some(Diagnostic::InsufficientCustomData {
                required: length,
                present: available,
                length_overflow: self.diagnostic == Some(Diagnostic::NumberOutOfRange),
            });
            return Err(ParseError);
        };
        let start = self.offset;
        self.offset += size;
        self.close()?;
        Ok(&self.input[start..start + size])
    }

    fn object_count(&mut self, colon: usize) -> (i64, usize) {
        let start = colon + 1;
        let (end, _) = signed_end(self.input, start);
        self.offset = end;
        (self.signed(start, end), end)
    }

    fn open_object_body(&mut self, end: usize) -> Result<(), ParseError> {
        if self.input.get(end) != Some(&b':') {
            return Err(ParseError);
        }
        self.offset += 1;
        if self.input.get(self.offset) != Some(&b'{') {
            return Err(ParseError);
        }
        self.offset += 1;
        Ok(())
    }

    fn enum_case(&mut self) -> Result<Token<'a>, ParseError> {
        let (start, len) = self.quoted_start(false)?;
        let end = start + len;
        self.quoted_end(end, b';')?;
        Ok(Token::Enum(&self.input[start..end]))
    }

    fn reference(&mut self, kind: u8) -> Result<Token<'a>, ParseError> {
        let start = self.offset + 2;
        let end = digits_end(self.input, start);
        if end == start || self.input.get(end) != Some(&b';') {
            return Err(ParseError);
        }
        self.offset = end + 1;
        let slot = unsigned(&self.input[start..end]);
        if kind == b'R' {
            Ok(Token::Reference(slot))
        } else {
            Ok(Token::ObjectReference(slot))
        }
    }
}
