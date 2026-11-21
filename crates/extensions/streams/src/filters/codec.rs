//! `convert.base64-encode` retains incomplete triples across writes.

use std::io;

use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use phera_extension_api::{
    ArrayKey, IconvConvert, IconvConverter, IconvFailure, IconvStream, Value,
};

use super::{
    Flush, MAX_BUCKET, bounded,
    compression::Zlib,
    dechunk::Dechunk,
    quoted::{QuotedDecode, QuotedEncode},
};

pub(super) enum Codec {
    Rot13,
    Upper,
    Lower,
    Base64Encode(Base64Encode),
    Base64Decode(Vec<u8>),
    Iconv(Iconv),
    Zlib(Zlib),
    Consumed { start: Option<u64>, count: u64 },
    Dechunk(Dechunk),
    QuotedEncode(QuotedEncode),
    QuotedDecode(QuotedDecode),
}

impl Codec {
    pub fn new(name: &[u8], params: &Value, iconv: Option<IconvConverter>) -> Option<Self> {
        Some(match name {
            b"string.rot13" => Self::Rot13,
            b"string.toupper" => Self::Upper,
            b"string.tolower" => Self::Lower,
            b"convert.base64-encode" => Self::Base64Encode(Base64Encode::new(params)),
            b"convert.base64-decode" => Self::Base64Decode(Vec::new()),
            b"zlib.inflate" => Self::Zlib(Zlib::inflate(integer(params, "window").unwrap_or(-15))?),
            b"zlib.deflate" => {
                let level = match params {
                    Value::Int(level) => *level,
                    _ => integer(params, "level").unwrap_or(-1),
                };
                Self::Zlib(Zlib::deflate(
                    level,
                    integer(params, "window").unwrap_or(-15),
                )?)
            }
            b"consumed" => Self::Consumed {
                start: None,
                count: 0,
            },
            b"dechunk" => Self::Dechunk(Dechunk::default()),
            b"convert.quoted-printable-encode" => Self::QuotedEncode(QuotedEncode::new(params)),
            b"convert.quoted-printable-decode" => Self::QuotedDecode(QuotedDecode::new(params)),
            name => Self::Iconv(Iconv::parse(name, iconv?)?),
        })
    }

    /// Consumed passes buckets through and puts the backing cursor after its input on close.
    pub fn seek_after(&mut self, size: usize, flush: Flush, position: u64) -> Option<u64> {
        let Self::Consumed { start, count } = self else {
            return None;
        };
        let start = *start.get_or_insert(position);
        let end = start.saturating_add(*count);
        *count = count.saturating_add(size as u64);
        (flush == Flush::Closing).then_some(end)
    }

    pub fn apply(&mut self, mut data: Vec<u8>, flush: Flush) -> io::Result<Vec<u8>> {
        match self {
            Self::Rot13 => {
                for byte in &mut data {
                    *byte = match *byte {
                        b'A'..=b'M' | b'a'..=b'm' => *byte + 13,
                        b'N'..=b'Z' | b'n'..=b'z' => *byte - 13,
                        _ => *byte,
                    };
                }
                Ok(data)
            }
            Self::Upper => {
                data.make_ascii_uppercase();
                Ok(data)
            }
            Self::Lower => {
                data.make_ascii_lowercase();
                Ok(data)
            }
            Self::Base64Encode(codec) => codec.run(data, flush),
            Self::Base64Decode(remainder) => decode_base64(remainder, data, flush),
            Self::Zlib(codec) => codec.run(&data, flush),
            Self::Iconv(codec) => codec.run(data, flush),
            Self::Consumed { .. } => Ok(data),
            Self::Dechunk(codec) => Ok(codec.run(data, flush)),
            Self::QuotedEncode(codec) => codec.run(data, flush),
            Self::QuotedDecode(codec) => codec.run(data, flush),
        }
    }
}

pub(super) fn field<'a>(params: &'a Value, key: &str) -> Option<&'a Value> {
    let Value::Array(params) = params else {
        return None;
    };
    params.get(&ArrayKey::str(key))
}

pub(super) fn integer(params: &Value, key: &str) -> Option<i64> {
    match field(params, key)? {
        Value::Int(value) => Some(*value),
        Value::String(value) => std::str::from_utf8(value.as_bytes()).ok()?.parse().ok(),
        _ => None,
    }
}

pub(super) struct Base64Encode {
    remainder: Vec<u8>,
    width: usize,
    line_break: Vec<u8>,
    column: usize,
}

impl Base64Encode {
    fn new(params: &Value) -> Self {
        let width = integer(params, "line-length")
            .and_then(|size| usize::try_from(size).ok())
            .unwrap_or(0)
            / 4
            * 4;
        let line_break = if width == 0 {
            Vec::new()
        } else if let Some(Value::String(value)) = field(params, "line-break-chars") {
            value.as_bytes()[..value.as_bytes().len().min(MAX_BUCKET)].to_vec()
        } else {
            b"\r\n".to_vec()
        };
        Self {
            remainder: Vec::new(),
            width,
            line_break,
            column: 0,
        }
    }

    fn run(&mut self, bytes: Vec<u8>, flush: Flush) -> io::Result<Vec<u8>> {
        let mut data = std::mem::take(&mut self.remainder);
        bounded(data.len() + bytes.len())?;
        data.extend(bytes);
        if flush != Flush::Closing {
            let end = data.len() / 3 * 3;
            self.remainder = data.split_off(end);
        }
        let encoded_len = data.len().div_ceil(3) * 4;
        bounded(encoded_len)?;
        let encoded = STANDARD.encode(data).into_bytes();
        if self.width == 0 {
            return Ok(encoded);
        }
        let mut output = Vec::with_capacity(encoded_len);
        for quartet in encoded.chunks(4) {
            if self.column == self.width {
                bounded(output.len() + self.line_break.len())?;
                output.extend_from_slice(&self.line_break);
                self.column = 0;
            }
            bounded(output.len() + quartet.len())?;
            output.extend_from_slice(quartet);
            self.column += quartet.len();
        }
        Ok(output)
    }
}

fn decode_base64(remainder: &mut Vec<u8>, data: Vec<u8>, flush: Flush) -> io::Result<Vec<u8>> {
    let mut result = Vec::new();
    for byte in data {
        if byte == b'=' || byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/') {
            remainder.push(byte);
        }
        if remainder.len() == 4 {
            let mut output = [0; 3];
            let written = STANDARD
                .decode_slice(&*remainder, &mut output)
                .map_err(invalid)?;
            result.extend_from_slice(&output[..written]);
            remainder.clear();
        }
    }
    if flush == Flush::Closing && remainder.len() >= 2 {
        result.extend(STANDARD_NO_PAD.decode(&*remainder).map_err(invalid)?);
        remainder.clear();
    }
    Ok(result)
}

pub(super) enum Iconv {
    Streaming(Box<dyn IconvStream>),
    Stateless(StatelessIconv),
}

pub(super) struct StatelessIconv {
    convert: IconvConvert,
    from: String,
    to: String,
    remainder: Vec<u8>,
}

impl Iconv {
    fn parse(name: &[u8], converter: IconvConverter) -> Option<Self> {
        let name = std::str::from_utf8(name.strip_prefix(b"convert.iconv.")?).ok()?;
        let split = name.find(['.', '/'])?;
        let (from, to) = (&name[..split], &name[split + 1..]);
        (converter.convert)(b"", from, to).ok()?;
        if let Some(stream) = (converter.stream)(from, to) {
            return Some(Self::Streaming(stream));
        }
        Some(Self::Stateless(StatelessIconv {
            convert: converter.convert,
            from: from.into(),
            to: to.into(),
            remainder: Vec::new(),
        }))
    }

    fn run(&mut self, data: Vec<u8>, flush: Flush) -> io::Result<Vec<u8>> {
        match self {
            Self::Streaming(stream) => stream.convert(&data, flush != Flush::No, MAX_BUCKET),
            Self::Stateless(iconv) => iconv.run(data, flush),
        }
    }
}

impl StatelessIconv {
    fn run(&mut self, data: Vec<u8>, flush: Flush) -> io::Result<Vec<u8>> {
        let mut bytes = std::mem::take(&mut self.remainder);
        bounded(bytes.len() + data.len())?;
        bytes.extend(data);
        match (self.convert)(&bytes, &self.from, &self.to) {
            Ok(output) => Ok(output),
            Err(IconvFailure::IncompleteCharacter) if flush != Flush::Closing => {
                for retained in 1..=bytes.len().min(8) {
                    let boundary = bytes.len() - retained;
                    if let Ok(output) = (self.convert)(&bytes[..boundary], &self.from, &self.to) {
                        self.remainder = bytes.split_off(boundary);
                        return Ok(output);
                    }
                }
                Err(invalid("invalid byte sequence"))
            }
            Err(_) => Err(invalid("invalid byte sequence")),
        }
    }
}

fn invalid(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
