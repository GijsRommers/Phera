//! Integer bases, decimal grouping, and PHP's numeric filter whitespace.
//!
//! ```php
//! filter_var('012', FILTER_VALIDATE_INT, FILTER_FLAG_ALLOW_OCTAL); // 10
//! filter_var('01', FILTER_VALIDATE_INT); // false
//! filter_var('1,234.50', FILTER_VALIDATE_FLOAT, FILTER_FLAG_ALLOW_THOUSAND); // 1234.5
//! ```

use phera_extension_api::value::{NumVal, parse_numeric};
use phera_extension_api::{Context, NativeError};

use crate::constants::*;
use crate::options::Options;

fn trim(input: &[u8]) -> &[u8] {
    let Some(start) = input.iter().position(|byte| !filter_space(byte)) else {
        return &[];
    };
    let end = input
        .iter()
        .rposition(|byte| !filter_space(byte))
        .unwrap_or(start);
    &input[start..=end]
}

fn filter_space(byte: &u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | 0x0b | b'\n')
}

pub fn boolean(input: &[u8]) -> Option<bool> {
    let text = trim(input);
    if [b"1".as_slice(), b"true", b"on", b"yes"]
        .iter()
        .any(|word| text.eq_ignore_ascii_case(word))
    {
        Some(true)
    } else if [b"".as_slice(), b"0", b"false", b"off", b"no"]
        .iter()
        .any(|word| text.eq_ignore_ascii_case(word))
    {
        Some(false)
    } else {
        None
    }
}

pub fn integer(
    context: &mut dyn Context,
    input: &[u8],
    options: &Options,
) -> Result<Option<i64>, NativeError> {
    let min = options.int(context, "min_range")?;
    let max = options.int(context, "max_range")?;
    Ok(parse_int(trim(input), options).filter(|&number| {
        min.is_none_or(|min| number >= min) && max.is_none_or(|max| number <= max)
    }))
}

fn parse_int(input: &[u8], options: &Options) -> Option<i64> {
    let text = std::str::from_utf8(input).ok()?;
    match input {
        b"0" | b"+0" | b"-0" => Some(0),
        [b'0', b'x' | b'X', digits @ ..] if options.flags.has(ALLOW_HEX) => radix(digits, 16),
        [b'0', b'o' | b'O', digits @ ..] if options.flags.has(ALLOW_OCTAL) => radix(digits, 8),
        [b'0', digits @ ..] if options.flags.has(ALLOW_OCTAL) => radix(digits, 8),
        [b'0', ..] | [b'+' | b'-', b'0', ..] => None,
        _ => text.parse().ok(),
    }
}

fn radix(digits: &[u8], base: u32) -> Option<i64> {
    // Hex and octal in PHP use the full unsigned word, interpreted as signed.
    if !digits.first()?.is_ascii_hexdigit() {
        return None;
    }
    u64::from_str_radix(std::str::from_utf8(digits).ok()?, base)
        .ok()
        .map(|value| value as i64)
}

pub fn float(
    context: &mut dyn Context,
    input: &[u8],
    options: &Options,
) -> Result<Option<f64>, NativeError> {
    let input = trim(input);
    if input.is_empty() {
        return Ok(None);
    }

    let decimal = options.string("decimal");
    let separator = match decimal.as_ref().map(|value| value.as_bytes()) {
        None => b'.',
        Some(&[byte]) => byte,
        Some(_) => return Err(options.invalid("\"decimal\" option must be one character long")),
    };

    let thousand = options.string("thousand");
    let grouping = match thousand.as_ref().map(|value| value.as_bytes()) {
        Some([]) => return Err(options.invalid("\"thousand\" option must not be empty")),
        Some(bytes) => bytes.split(|&byte| byte == 0).next().unwrap_or_default(),
        None => b"',.",
    };

    let min = options.float(context, "min_range")?;
    let max = options.float(context, "max_range")?;
    let grouping = options.flags.has(ALLOW_THOUSAND).then_some(grouping);
    Ok(parse_float(input, separator, grouping).filter(|&number| {
        !min.is_some_and(|min| number < min) && !max.is_some_and(|max| number > max)
    }))
}

fn parse_float(input: &[u8], decimal: u8, grouping: Option<&[u8]>) -> Option<f64> {
    let mut bytes = input.iter().copied().peekable();
    let mut number = Vec::with_capacity(input.len());
    if bytes.peek().is_some_and(|byte| matches!(byte, b'+' | b'-')) {
        number.extend(bytes.next());
    }

    let mut first = true;
    loop {
        let previous = number.len();
        append_digits(&mut number, &mut bytes);
        let digits = number.len() - previous;
        match bytes.peek().copied() {
            None => {
                if !first && digits != 3 {
                    return None;
                }
                break;
            }
            Some(byte) if byte == decimal || matches!(byte, b'e' | b'E') => {
                if !first && digits != 3 {
                    return None;
                }
                if byte == decimal {
                    number.push(b'.');
                    bytes.next();
                    append_digits(&mut number, &mut bytes);
                }
                if let Some(exponent) = bytes.next_if(|byte| matches!(byte, b'e' | b'E')) {
                    number.push(exponent);
                    if let Some(sign) = bytes.next_if(|byte| matches!(byte, b'+' | b'-')) {
                        number.push(sign);
                    }
                    append_digits(&mut number, &mut bytes);
                }
                if bytes.next().is_some() {
                    return None;
                }
                break;
            }
            Some(byte) => {
                let valid_count = if first {
                    (1..=3).contains(&digits)
                } else {
                    digits == 3
                };
                if !valid_count || !grouping.is_some_and(|seps| byte == 0 || seps.contains(&byte)) {
                    return None;
                }
                first = false;
                bytes.next();
            }
        }
    }

    let parsed = parse_numeric(&number);
    if !parsed.exact {
        return None;
    }
    match parsed.value? {
        NumVal::Int(value) => Some(value as f64),
        NumVal::Float(value) => {
            let underflow = value == 0.0 && number.iter().any(|byte| matches!(byte, b'1'..=b'9'));
            (value.is_finite() && !underflow).then_some(value)
        }
    }
}

fn append_digits(target: &mut Vec<u8>, bytes: &mut std::iter::Peekable<impl Iterator<Item = u8>>) {
    while let Some(digit) = bytes.next_if(u8::is_ascii_digit) {
        target.push(digit);
    }
}
