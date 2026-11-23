//! `convert.quoted-printable-decode` carries split hex escapes into the next bucket.

use std::io;

use phera_extension_api::{Value, value::to_bool};

use super::{
    Flush, MAX_BUCKET, bounded,
    codec::{field, integer},
    dechunk::hex,
};

pub(super) struct QuotedEncode {
    column: usize,
    width: usize,
    line_break: Vec<u8>,
    binary: bool,
    force_first: bool,
    remainder: Vec<u8>,
}

impl QuotedEncode {
    pub fn new(params: &Value) -> Self {
        let width = integer(params, "line-length")
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n >= 4)
            .unwrap_or(0);
        let line_break = match field(params, "line-break-chars") {
            Some(Value::String(value)) => {
                value.as_bytes()[..value.as_bytes().len().min(MAX_BUCKET)].to_vec()
            }
            _ => b"\r\n".to_vec(),
        };
        Self {
            column: 0,
            width,
            line_break,
            remainder: Vec::new(),
            binary: field(params, "binary").is_some_and(to_bool),
            force_first: field(params, "force-encode-first").is_some_and(to_bool),
        }
    }

    pub fn run(&mut self, data: Vec<u8>, flush: Flush) -> io::Result<Vec<u8>> {
        let mut input = std::mem::take(&mut self.remainder);
        bounded(input.len() + data.len())?;
        input.extend(data);
        let mut output = Vec::new();
        let mut position = 0;
        while position < input.len() {
            let remaining = &input[position..];
            let byte = remaining[0];
            if !self.binary
                && !self.line_break.is_empty()
                && remaining.starts_with(&self.line_break)
            {
                bounded(output.len() + self.line_break.len())?;
                output.extend(&self.line_break);
                self.column = 0;
                position += self.line_break.len();
                continue;
            }
            if flush != Flush::Closing
                && (!self.binary && self.line_break.starts_with(remaining)
                    || remaining.len() == 1 && matches!(byte, b' ' | b'\t'))
            {
                self.remainder = remaining.to_vec();
                break;
            }
            let space = matches!(byte, b' ' | b'\t');
            let printable = (33..=126).contains(&byte) && byte != b'=';
            let at_end = remaining.len() == 1
                || !self.line_break.is_empty() && remaining[1..].starts_with(&self.line_break);
            let encode = !(printable || space && !at_end) || self.force_first && self.column == 0;
            let size = if encode { 3 } else { 1 };
            if self.width != 0 && self.column + size >= self.width {
                bounded(output.len() + 1 + self.line_break.len())?;
                output.push(b'=');
                output.extend(&self.line_break);
                self.column = 0;
            }
            bounded(output.len() + size)?;
            if encode {
                const HEX: &[u8] = b"0123456789ABCDEF";
                output.extend([b'=', HEX[(byte >> 4) as usize], HEX[(byte & 15) as usize]]);
            } else {
                output.push(byte);
            }
            position += 1;
            self.column += size;
        }
        Ok(output)
    }
}

pub(super) struct QuotedDecode {
    line_break: Vec<u8>,
    remainder: Vec<u8>,
}

impl QuotedDecode {
    pub fn new(params: &Value) -> Self {
        let line_break = match field(params, "line-break-chars") {
            Some(Value::String(value)) => {
                value.as_bytes()[..value.as_bytes().len().min(MAX_BUCKET)].to_vec()
            }
            _ => Vec::new(),
        };
        Self {
            line_break,
            remainder: Vec::new(),
        }
    }

    pub fn run(&mut self, data: Vec<u8>, flush: Flush) -> io::Result<Vec<u8>> {
        let mut input = std::mem::take(&mut self.remainder);
        bounded(input.len() + data.len())?;
        input.extend(data);
        let mut output = Vec::new();
        let mut remaining = input.as_slice();
        while let Some((&byte, rest)) = remaining.split_first() {
            if byte != b'=' {
                output.push(byte);
                remaining = rest;
                continue;
            }
            let soft = if self.line_break.is_empty() {
                rest.strip_prefix(b"\r\n")
                    .or_else(|| rest.strip_prefix(b"\n"))
            } else {
                rest.strip_prefix(self.line_break.as_slice())
            };
            if let Some(after) = soft {
                remaining = after;
            } else if let [high, low, after @ ..] = rest
                && let (Some(high), Some(low)) = (hex(*high), hex(*low))
            {
                output.push((high * 16 + low) as u8);
                remaining = after;
            } else if flush != Flush::Closing
                && (rest.len() < 2 || self.line_break.starts_with(rest))
            {
                self.remainder.extend(remaining);
                break;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid quoted-printable data",
                ));
            }
        }
        Ok(output)
    }
}
