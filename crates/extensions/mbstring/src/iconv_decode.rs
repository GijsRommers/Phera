//! MIME decoding respects folding and flags: `iconv_mime_decode("a\r\n b") === 'a b'`.
use crate::{iconv, mime};
use phera_extension_api::Context;

pub struct Mode {
    strict: bool,
    continue_on_error: bool,
}
impl Mode {
    pub fn new(flags: i64) -> Self {
        Self {
            strict: flags & 1 != 0,
            continue_on_error: flags & 2 != 0,
        }
    }
}
fn unfold(bytes: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if matches!(bytes[index], b'\r' | b'\n') {
            if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            index += 1;
            if !matches!(bytes.get(index), Some(b' ' | b'\t')) {
                break;
            }
            while matches!(bytes.get(index), Some(b' ' | b'\t')) {
                index += 1;
            }
            while matches!(result.last(), Some(b' ' | b'\t')) {
                result.pop();
            }
            result.push(b' ');
        } else {
            result.push(bytes[index]);
            index += 1;
        }
    }
    result
}
struct Word<'a> {
    charset: &'a str,
    bytes: Vec<u8>,
    length: usize,
}
fn word(bytes: &[u8]) -> Option<Word<'_>> {
    let bytes = bytes.strip_prefix(b"=?")?;
    let end = bytes.iter().position(|b| *b == b'?')?;
    let charset = std::str::from_utf8(&bytes[..end]).ok()?;
    let tail = &bytes[end + 1..];
    if tail.get(1) != Some(&b'?') || !matches!(tail.first(), Some(b'B' | b'b' | b'Q' | b'q')) {
        return None;
    }
    let close = tail[2..].windows(2).position(|w| w == b"?=")? + 2;
    Some(Word {
        charset: charset.split('*').next()?,
        bytes: mime::decoded(&tail[2..close], mime::transfer(tail))?,
        length: 2 + end + 1 + close + 2,
    })
}
pub fn decode(
    ctx: &mut dyn Context,
    input: &[u8],
    target: &str,
    mode: &Mode,
    name: &str,
) -> Option<Vec<u8>> {
    let input = unfold(input);
    let mut result = Vec::new();
    let mut index = 0;
    let mut previous = false;
    while index < input.len() {
        if let Some(word) = word(&input[index..]) {
            let end = index + word.length;
            let left = index == 0 || input[index - 1].is_ascii_whitespace();
            let right = end == input.len() || input[end].is_ascii_whitespace();
            if !mode.strict || left && right {
                match iconv_native::convert(&word.bytes, word.charset, target) {
                    Ok(bytes) => {
                        result.extend(bytes);
                        previous = true;
                    }
                    Err(error) if mode.continue_on_error => {
                        let trim = usize::from(
                            end == input.len()
                                && matches!(error, iconv_native::ConvertError::InvalidInput),
                        );
                        result.extend_from_slice(&input[index..end - trim]);
                        previous = false;
                    }
                    Err(iconv_native::ConvertError::UnknownConversion) => {
                        ctx.warning(format!(
                            "{name}(): Wrong encoding, conversion from \"???\" to \"{target}\" \
                             is not allowed"
                        ));
                        return None;
                    }
                    Err(iconv_native::ConvertError::InvalidInput) => {
                        return iconv::convert_bytes(ctx, &word.bytes, word.charset, target, name);
                    }
                }
                index = end;
                continue;
            }
        }
        if previous && input[index].is_ascii_whitespace() {
            let end = input[index..]
                .iter()
                .position(|b| !b.is_ascii_whitespace())
                .map_or(input.len(), |n| index + n);
            if word(&input[end..]).is_some() {
                index = end;
                continue;
            }
        }
        result.extend(iconv::convert_bytes(
            ctx,
            &input[index..index + 1],
            "ASCII",
            target,
            name,
        )?);
        index += 1;
        previous = false;
    }
    Some(result)
}
