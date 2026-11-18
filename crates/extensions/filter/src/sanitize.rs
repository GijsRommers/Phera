//! The byte-oriented sanitizers preserve or escape individual bytes.
//!
//! ```php
//! filter_var('a b~', FILTER_SANITIZE_ENCODED); // 'a%20b%7E'
//! filter_var('< &', FILTER_SANITIZE_SPECIAL_CHARS); // '&#60; &#38;'
//! filter_var('EUR -1.20', FILTER_SANITIZE_NUMBER_INT); // '-120'
//! ```

use phera_extension_api::{PhpString, Value};

use crate::constants::*;
use crate::options::Flags;

const HEX: &[u8; 16] = b"0123456789ABCDEF";

pub fn raw(input: &PhpString, flags: Flags) -> Value {
    if input.is_empty() && flags.has(EMPTY_STRING_NULL) {
        return Value::Null;
    }
    if !flags.has(STRIP_LOW | STRIP_HIGH | STRIP_BACKTICK | ENCODE_LOW | ENCODE_HIGH | ENCODE_AMP) {
        return Value::String(input.clone());
    }
    Value::from(entities(input.as_bytes(), flags, EntityMode::Raw))
}

pub fn special(input: &[u8], flags: Flags) -> Vec<u8> {
    entities(input, flags, EntityMode::Special)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EntityMode {
    Raw,
    Special,
}

fn entities(input: &[u8], flags: Flags, mode: EntityMode) -> Vec<u8> {
    let mut result = Vec::with_capacity(input.len());
    for byte in stripped(input, flags) {
        let encode = match mode {
            EntityMode::Special => byte < 32 || matches!(byte, b'\'' | b'"' | b'<' | b'>' | b'&'),
            EntityMode::Raw => {
                (byte < 32 && flags.has(ENCODE_LOW)) || (byte == b'&' && flags.has(ENCODE_AMP))
            }
        } || (byte >= 127 && flags.has(ENCODE_HIGH));

        if encode {
            result.extend_from_slice(b"&#");
            result.extend_from_slice(byte.to_string().as_bytes());
            result.push(b';');
        } else {
            result.push(byte);
        }
    }
    result
}

pub fn encoded(input: &[u8], flags: Flags) -> Vec<u8> {
    let mut result = Vec::with_capacity(input.len());
    for byte in stripped(input, flags) {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_') {
            result.push(byte);
        } else {
            result.extend([
                b'%',
                HEX[usize::from(byte >> 4)],
                HEX[usize::from(byte & 15)],
            ]);
        }
    }
    result
}

fn stripped(input: &[u8], flags: Flags) -> impl Iterator<Item = u8> {
    input.iter().copied().filter(move |&byte| {
        !(byte < 32 && flags.has(STRIP_LOW)
            || byte >= 127 && flags.has(STRIP_HIGH)
            || byte == b'`' && flags.has(STRIP_BACKTICK))
    })
}

pub fn email(input: &[u8]) -> Vec<u8> {
    input
        .iter()
        .copied()
        .filter(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-=?^_`{|}~@.[]".contains(byte))
        .collect()
}

pub fn url_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"$-_.+!*'(),{}|\\^~[]`<>#%\";/?:@&=".contains(&byte)
}

pub fn url(input: &[u8]) -> Vec<u8> {
    input
        .iter()
        .copied()
        .filter(|&byte| url_byte(byte))
        .collect()
}

pub fn number_int(input: &[u8]) -> Vec<u8> {
    input
        .iter()
        .copied()
        .filter(|&byte| number_byte(byte))
        .collect()
}

pub fn number_float(input: &[u8], flags: Flags) -> Vec<u8> {
    input
        .iter()
        .copied()
        .filter(|&byte| {
            number_byte(byte)
                || byte == b'.' && flags.has(ALLOW_FRACTION)
                || byte == b',' && flags.has(ALLOW_THOUSAND)
                || matches!(byte, b'e' | b'E') && flags.has(ALLOW_SCIENTIFIC)
        })
        .collect()
}

fn number_byte(byte: u8) -> bool {
    byte.is_ascii_digit() || matches!(byte, b'+' | b'-')
}

pub fn slashes(input: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(input.len());
    for &byte in input {
        match byte {
            0 => result.extend_from_slice(b"\\0"),
            b'\\' | b'\'' | b'"' => result.extend([b'\\', byte]),
            _ => result.push(byte),
        }
    }
    result
}
