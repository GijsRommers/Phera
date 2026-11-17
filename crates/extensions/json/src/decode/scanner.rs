//! A byte scanner retaining PHP's distinct syntax, UTF-8 and UTF-16 failures.
use crate::error::Error;
use crate::flags::{DecodeFlags, InvalidUtf8};
use phera_extension_api::Value;
/// Tokens separate grammar decisions from byte validation.
#[derive(Debug)]
pub(super) enum Token {
    Value(Value),
    Open(bool),
    Close(bool),
    Colon,
    Comma,
    End,
}
/// Cursor movements are bounded by checked byte reads.
pub(super) struct Scanner<'a> {
    pub bytes: &'a [u8],
    pub pos: usize,
    pub flags: DecodeFlags,
}
impl Scanner<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn take(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.pos += 1;
        Some(byte)
    }
    /// Outside strings PHP accepts only the four JSON whitespace characters.
    pub fn next(&mut self) -> Result<Token, Error> {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
        Ok(match self.take() {
            None => Token::End,
            Some(b'[') => Token::Open(false),
            Some(b'{') => Token::Open(true),
            Some(b']') => Token::Close(false),
            Some(b'}') => Token::Close(true),
            Some(b':') => Token::Colon,
            Some(b',') => Token::Comma,
            Some(b'"') => Token::Value(Value::from(self.string()?)),
            Some(byte @ (b'-' | b'0'..=b'9')) => {
                self.pos -= 1;
                Token::Value(self.number(byte)?)
            }
            Some(b'n') => {
                self.literal(b"ull")?;
                Token::Value(Value::Null)
            }
            Some(b't') => {
                self.literal(b"rue")?;
                Token::Value(Value::Bool(true))
            }
            Some(b'f') => {
                self.literal(b"alse")?;
                Token::Value(Value::Bool(false))
            }
            Some(byte) if byte < 32 => return Err(Error::Control),
            Some(byte) if byte >= 128 => {
                self.pos -= 1;
                self.utf8()?;
                return Err(Error::Syntax);
            }
            _ => return Err(Error::Syntax),
        })
    }
    fn literal(&mut self, suffix: &[u8]) -> Result<(), Error> {
        for &expected in suffix {
            if self.take() != Some(expected) {
                return Err(Error::Syntax);
            }
        }
        Ok(())
    }
    /// Consume one valid scalar, or exactly one invalid byte (PHP scanner semantics).
    fn utf8(&mut self) -> Result<Vec<u8>, Error> {
        let first = self.peek().ok_or(Error::Syntax)?;
        let length = match first {
            0..=127 => 1,
            194..=223 => 2,
            224..=239 => 3,
            240..=244 => 4,
            _ => 0,
        };
        let candidate = self.bytes.get(self.pos..self.pos.saturating_add(length));
        if length != 0
            && let Some(bytes) = candidate
            && std::str::from_utf8(bytes).is_ok()
        {
            self.pos += length;
            return Ok(bytes.to_vec());
        }
        self.pos += 1;
        Err(Error::Utf8)
    }
    /// Decode escapes before inserting property names, including embedded NULs.
    fn string(&mut self) -> Result<Vec<u8>, Error> {
        let mut output = Vec::new();
        loop {
            match self.peek() {
                None => return Err(Error::Control),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(output);
                }
                Some(b'\\') => {
                    self.pos += 1;
                    self.escape(&mut output)?;
                }
                Some(0..=31) => return Err(Error::Control),
                Some(_) => match self.utf8() {
                    Ok(bytes) => output.extend(bytes),
                    Err(error) => match self.flags.invalid_utf8 {
                        InvalidUtf8::Substitute => output.extend_from_slice("\u{fffd}".as_bytes()),
                        InvalidUtf8::Ignore => {}
                        InvalidUtf8::Fail => return Err(error),
                    },
                },
            }
        }
    }
    fn escape(&mut self, output: &mut Vec<u8>) -> Result<(), Error> {
        let byte = match self.take() {
            Some(b'"') => b'"',
            Some(b'\\') => b'\\',
            Some(b'/') => b'/',
            Some(b'b') => 8,
            Some(b'f') => 12,
            Some(b'n') => 10,
            Some(b'r') => 13,
            Some(b't') => 9,
            Some(b'u') => {
                let scalar = self.unicode()?;
                let mut bytes = [0; 4];
                output.extend_from_slice(scalar.encode_utf8(&mut bytes).as_bytes());
                return Ok(());
            }
            _ => return Err(Error::Syntax),
        };
        output.push(byte);
        Ok(())
    }
    fn hex(&mut self) -> Result<u32, Error> {
        let mut number = 0;
        for _ in 0..4 {
            let digit = char::from(self.take().ok_or(Error::Syntax)?)
                .to_digit(16)
                .ok_or(Error::Syntax)?;
            number = number * 16 + digit;
        }
        Ok(number)
    }
    /// PHP distinguishes malformed escapes from valid but unpaired surrogates.
    fn unicode(&mut self) -> Result<char, Error> {
        let first = self.hex()?;
        let scalar = if (0xd800..=0xdbff).contains(&first) {
            if self.take() != Some(b'\\') || self.take() != Some(b'u') {
                return Err(Error::Utf16);
            }
            let second = self.hex().map_err(|_| Error::Utf16)?;
            if !(0xdc00..=0xdfff).contains(&second) {
                return Err(Error::Utf16);
            }
            0x10000 + ((first - 0xd800) << 10) + second - 0xdc00
        } else {
            first
        };
        char::from_u32(scalar).ok_or(Error::Utf16)
    }
    /// Integer overflow becomes a float unless BIGINT_AS_STRING was requested.
    fn number(&mut self, _: u8) -> Result<Value, Error> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        match self.take() {
            Some(b'0') => {}
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(Error::Syntax),
        }
        let mut integer = true;
        if self.peek() == Some(b'.') {
            integer = false;
            self.pos += 1;
            if !self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                return Err(Error::Syntax);
            }
            self.digits();
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            integer = false;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if !self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                return Err(Error::Syntax);
            }
            self.digits();
        }
        let text = std::str::from_utf8(self.bytes.get(start..self.pos).ok_or(Error::Syntax)?)
            .map_err(|_| Error::Syntax)?;
        if integer {
            if let Ok(number) = text.parse::<i64>() {
                return Ok(Value::Int(number));
            }
            if self.flags.bigint_as_string {
                return Ok(Value::from(text));
            }
        }
        text.parse::<f64>()
            .map(Value::Float)
            .map_err(|_| Error::Syntax)
    }
    fn digits(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.pos += 1;
        }
    }
}
