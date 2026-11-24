//! `convert.iconv.UTF-8.ISO-2022-JP` carries the current character set across buckets.

use std::{borrow::Cow, io};

use encoding_rs::{Decoder, DecoderResult, Encoder, EncoderResult, ISO_2022_JP};
use phera_extension_api::IconvStream;

enum Converter {
    Decode(Decoder),
    Encode {
        encoder: Encoder,
        incomplete: Vec<u8>,
    },
}

pub(super) fn open(from: &str, to: &str) -> Option<Box<dyn IconvStream>> {
    let utf8 = |name: &str| name.eq_ignore_ascii_case("UTF-8") || name.eq_ignore_ascii_case("UTF8");
    let iso2022 = |name: &str| name.eq_ignore_ascii_case("ISO-2022-JP");
    let converter = if iso2022(from) && utf8(to) {
        Converter::Decode(decoder())
    } else if utf8(from) && iso2022(to) {
        Converter::Encode {
            encoder: ISO_2022_JP.new_encoder(),
            incomplete: Vec::new(),
        }
    } else {
        return None;
    };
    Some(Box::new(converter))
}

impl IconvStream for Converter {
    fn convert(&mut self, bytes: &[u8], flush: bool, limit: usize) -> io::Result<Vec<u8>> {
        match self {
            Self::Decode(current) => {
                let result = decode(current, bytes, flush, limit)?;
                if flush {
                    *current = decoder();
                }
                Ok(result)
            }
            Self::Encode {
                encoder,
                incomplete,
            } => {
                let bytes = if incomplete.is_empty() {
                    Cow::Borrowed(bytes)
                } else {
                    let mut prefix = std::mem::take(incomplete);
                    prefix.extend_from_slice(bytes);
                    Cow::Owned(prefix)
                };
                let text = match std::str::from_utf8(&bytes) {
                    Ok(text) => text,
                    Err(error) if !flush && error.error_len().is_none() => {
                        let (complete, tail) = bytes.split_at(error.valid_up_to());
                        incomplete.extend_from_slice(tail);
                        std::str::from_utf8(complete).expect("the valid UTF-8 prefix")
                    }
                    Err(_) => return Err(invalid()),
                };
                let result = encode(encoder, text, flush, limit)?;
                if flush {
                    *encoder = ISO_2022_JP.new_encoder();
                }
                Ok(result)
            }
        }
    }
}

fn decoder() -> Decoder {
    ISO_2022_JP.new_decoder_without_bom_handling()
}

fn decode(
    decoder: &mut Decoder,
    mut bytes: &[u8],
    flush: bool,
    limit: usize,
) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let (result, read, written) =
            decoder.decode_to_utf8_without_replacement(bytes, &mut buffer, flush);
        emit(&mut output, &buffer[..written], limit)?;
        match result {
            DecoderResult::InputEmpty => return Ok(output),
            DecoderResult::Malformed(..) => return Err(invalid()),
            DecoderResult::OutputFull => bytes = &bytes[read..],
        }
    }
}

fn encode(encoder: &mut Encoder, mut text: &str, flush: bool, limit: usize) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let (result, read, written) =
            encoder.encode_from_utf8_without_replacement(text, &mut buffer, flush);
        emit(&mut output, &buffer[..written], limit)?;
        match result {
            EncoderResult::InputEmpty => return Ok(output),
            EncoderResult::Unmappable(_) => return Err(invalid()),
            EncoderResult::OutputFull => text = &text[read..],
        }
    }
}

fn emit(output: &mut Vec<u8>, bytes: &[u8], limit: usize) -> io::Result<()> {
    if output.len() + bytes.len() > limit {
        return Err(io::Error::other(
            "Stream filter output exceeds Phera's size limit",
        ));
    }
    output.extend_from_slice(bytes);
    Ok(())
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid byte sequence")
}
