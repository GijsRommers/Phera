//! `data:text/plain;base64,SGk=` provides a read-only stream containing "Hi".

use std::io::Cursor;

use base64::{
    Engine,
    engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
};

use crate::stream::{Backing, Stream, Wrapper};

pub fn open(data: &[u8], mode: &[u8], uri: &[u8]) -> Result<Stream, &'static str> {
    let comma = data
        .iter()
        .position(|&byte| byte == b',')
        .ok_or("rfc2397: no comma in URL")?;
    let (header, bytes) = (&data[..comma], &data[comma + 1..]);
    let encoded = data_encoding(header)?;
    let bytes = if encoded {
        let input: Vec<_> = bytes
            .iter()
            .copied()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect();
        let config = GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true);
        GeneralPurpose::new(&base64::alphabet::STANDARD, config)
            .decode(input)
            .map_err(|_| "rfc2397: unable to decode")?
    } else {
        let input: Vec<_> = bytes
            .iter()
            .map(|&byte| if byte == b'+' { b' ' } else { byte })
            .collect();
        percent_encoding::percent_decode(&input).collect()
    };
    let mut stream = Stream::new(
        Backing::Memory(Cursor::new(bytes)),
        Wrapper::Data,
        &mode[..mode.len().min(15)],
        uri,
    );
    stream.data_header = Some(header.to_vec());
    Ok(stream)
}

fn data_encoding(header: &[u8]) -> Result<bool, &'static str> {
    if header.is_empty() {
        return Ok(false);
    }
    if header == b";base64" {
        return Ok(true);
    }
    let mut fields = header.split(|byte| *byte == b';').peekable();
    if !fields.next().is_some_and(|media| media.contains(&b'/')) {
        return Err("rfc2397: illegal media type");
    }
    while let Some(field) = fields.next() {
        if field == b"base64" && fields.peek().is_none() {
            return Ok(true);
        }
        if !field.contains(&b'=') {
            return Err("rfc2397: illegal parameter");
        }
    }
    Ok(false)
}
