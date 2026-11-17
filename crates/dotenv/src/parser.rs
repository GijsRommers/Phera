//! Byte-oriented dotenv assignments, e.g. `PORT=8080` for PHP's `getenv("PORT")`.

mod expansion;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::Environment;

const VALUE_LIMIT: usize = 8 * 1024 * 1024;
pub(crate) const FILE_LIMIT: usize = 32 * 1024 * 1024;

#[derive(Debug)]
pub struct ParseError {
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub name: Option<String>,
    pub expected: &'static str,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}: ", self.file.display(), self.line, self.column)?;
        if let Some(name) = &self.name {
            write!(f, "{name}: ")?;
        }
        write!(f, "expected {}", self.expected)
    }
}

impl std::error::Error for ParseError {}

/// Parse one layer atomically. External names are locked even when their values are empty.
/// A bare name removes an earlier file assignment. Both inputs remain usable on error.
pub fn parse(
    path: &Path,
    bytes: &[u8],
    external: &Environment,
    previous: &Environment,
) -> Result<Environment, ParseError> {
    let mut parser = Parser {
        bytes: bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes),
        pos: 0,
        path,
        name: None,
        external,
        files: previous.clone(),
        expanded: 0,
    };
    if bytes.len() > FILE_LIMIT {
        return Err(parser.error("a file of at most 32 MiB"));
    }
    while parser.peek().is_some() {
        parser.name = None;
        parser.space();
        match parser.peek() {
            None => break,
            Some(b'#') => parser.comment(),
            Some(b'\n' | b'\r') => parser.newline(),
            _ => parser.assignment()?,
        }
    }
    Ok(parser.files)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    path: &'a Path,
    name: Option<&'a str>,
    external: &'a Environment,
    files: Environment,
    expanded: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
    fn rest(&self) -> &'a [u8] {
        &self.bytes[self.pos..]
    }
    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.pos += 1;
        }
    }
    fn comment(&mut self) {
        while !matches!(self.peek(), None | Some(b'\n' | b'\r')) {
            self.pos += 1;
        }
    }
    fn newline(&mut self) {
        self.pos += if self.rest().starts_with(b"\r\n") {
            2
        } else {
            1
        };
    }

    fn error(&self, expected: &'static str) -> ParseError {
        let (mut line, mut column) = (1, 1);
        for (i, &byte) in self.bytes[..self.pos].iter().enumerate() {
            match byte {
                b'\r' => {
                    line += 1;
                    column = 1;
                }
                b'\n' if i > 0 && self.bytes[i - 1] == b'\r' => {}
                b'\n' => {
                    line += 1;
                    column = 1;
                }
                _ => column += 1,
            }
        }
        ParseError {
            file: self.path.to_owned(),
            line,
            column,
            name: self.name.map(str::to_owned),
            expected,
        }
    }

    fn assignment(&mut self) -> Result<(), ParseError> {
        if self
            .rest()
            .strip_prefix(b"export")
            .is_some_and(|r| matches!(r.first(), Some(b' ' | b'\t')))
        {
            self.pos += 6;
            self.space();
        }
        let quote = self.peek().filter(|b| matches!(b, b'\'' | b'"'));
        self.pos += usize::from(quote.is_some());
        let start = self.pos;
        while self.peek().is_some_and(name_byte) {
            self.pos += 1;
        }
        let name = &self.bytes[start..self.pos];
        if !valid_name(name) {
            return Err(self.error("a valid variable name"));
        }
        if let Some(quote) = quote {
            if self.peek() != Some(quote) {
                return Err(self.error("a closing name quote"));
            }
            self.pos += 1;
        }
        self.name = std::str::from_utf8(name).ok();
        self.space();
        match self.peek() {
            Some(b'=') => {
                self.pos += 1;
                self.space();
                let value = self.value()?;
                if self.external.get(name).is_none() {
                    self.files.insert(name.to_vec(), value);
                }
            }
            None | Some(b'\n' | b'\r') => {
                self.files.0.shift_remove(name);
            }
            _ => return Err(self.error("'=' after the variable name")),
        }
        Ok(())
    }

    fn value(&mut self) -> Result<Vec<u8>, ParseError> {
        let quote = self.peek().filter(|b| matches!(b, b'\'' | b'"'));
        self.pos += usize::from(quote.is_some());
        let mut out = Vec::new();
        let mut keep = 0;
        let start = self.pos;
        while let Some(byte) = self.peek() {
            if Some(byte) == quote {
                self.pos += 1;
                self.space();
                if self.peek() == Some(b'#') {
                    self.comment();
                }
                if !matches!(self.peek(), None | Some(b'\n' | b'\r')) {
                    return Err(self.error("a comment or end of line after the closing quote"));
                }
                return Ok(out);
            }
            let comment = byte == b'#'
                && (self.pos == start || matches!(self.bytes[self.pos - 1], b' ' | b'\t'));
            if quote.is_none() && (comment || matches!(byte, b'\n' | b'\r')) {
                if byte == b'#' {
                    self.comment();
                }
                break;
            }
            match byte {
                0 => return Err(self.error("a value without a NUL byte")),
                b'$' if quote != Some(b'\'') => self.expand(&mut out)?,
                b'\\' if quote.is_none() => {
                    let start = self.pos;
                    while self.peek() == Some(b'\\') {
                        self.pos += 1;
                    }
                    let count = self.pos - start;
                    if self.peek() == Some(b'$') {
                        out.extend_from_slice(&self.bytes[start..start + count / 2]);
                        if count % 2 == 1 {
                            out.push(b'$');
                            self.pos += 1;
                        }
                    } else {
                        out.extend_from_slice(&self.bytes[start..self.pos]);
                    }
                }
                b'\\' if quote == Some(b'"') => {
                    self.pos += 1;
                    let escaped = match self.peek() {
                        Some(b'n') => b'\n',
                        Some(b'r') => b'\r',
                        Some(b't') => b'\t',
                        Some(b'v') => 11,
                        Some(b'f') => 12,
                        Some(b @ (b'"' | b'\\' | b'$')) => b,
                        Some(b) if b != 0 => {
                            out.push(b'\\');
                            b
                        }
                        _ => return Err(self.error("an escaped character")),
                    };
                    out.push(escaped);
                    self.pos += 1;
                }
                b'\r' | b'\n' => {
                    self.newline();
                    out.push(b'\n');
                }
                _ => {
                    self.pos += 1;
                    out.push(byte);
                }
            }
            if out.len() > VALUE_LIMIT {
                return Err(self.error("a value of at most 8 MiB"));
            }
            if !matches!(byte, b' ' | b'\t') {
                keep = out.len();
            }
        }
        if quote.is_some() {
            return Err(self.error("a closing value quote"));
        }
        out.truncate(keep);
        Ok(out)
    }
}

fn name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_') || !byte.is_ascii()
}

fn valid_name(name: &[u8]) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.is_ascii() {
        return name.iter().copied().all(name_byte);
    }
    static NAME: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A[\p{Ll}\p{Lu}\p{M}\p{N}_.]+\z").expect("valid name pattern")
    });
    std::str::from_utf8(name).is_ok_and(|s| NAME.is_match(s))
}
