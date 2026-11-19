//! `ini_parse_quantity('2M')` returns 2,097,152. PHP also accepts base
//! prefixes and preserves a best-effort result when it warns about bad input.

use std::fmt::Write;

fn whitespace(byte: &u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | b'\x0b' | b'\x0c')
}

fn escaped(bytes: &[u8]) -> String {
    let mut result = String::new();
    for &byte in bytes {
        match byte {
            b'\n' => result.push_str("\\n"),
            b'\r' => result.push_str("\\r"),
            b'\t' => result.push_str("\\t"),
            12 => result.push_str("\\f"),
            11 => result.push_str("\\v"),
            27 => result.push_str("\\e"),
            b'\\' => result.push_str("\\\\"),
            32..=126 => result.push(char::from(byte)),
            _ => {
                let _ = write!(result, "\\x{byte:02X}");
            }
        }
    }
    result
}

fn no_digits(input: &[u8], reason: &str) -> (i64, Option<String>) {
    let input = escaped(input);
    (
        0,
        Some(format!(
            "Invalid quantity \"{input}\": {reason}, interpreting as \"0\" for backwards compatibility"
        )),
    )
}

fn multiplier(byte: u8) -> Option<i64> {
    match byte.to_ascii_lowercase() {
        b'k' => Some(1 << 10),
        b'm' => Some(1 << 20),
        b'g' => Some(1 << 30),
        _ => None,
    }
}

/// Return PHP's signed result and its optional warning, without a caller prefix.
pub fn parse_quantity(input: &[u8]) -> (i64, Option<String>) {
    let leading = input.iter().take_while(|byte| whitespace(byte)).count();
    let text = &input[leading..];
    let trailing = text
        .iter()
        .rev()
        .take_while(|byte| whitespace(byte))
        .count();
    let text = &text[..text.len() - trailing];
    if text.is_empty() {
        return (0, None);
    }
    let negative = text.starts_with(b"-");
    let sign = usize::from(text.starts_with(b"-") || text.starts_with(b"+"));
    let mut digits = &text[sign..];
    if !digits.first().is_some_and(u8::is_ascii_digit) {
        return no_digits(input, "no valid leading digits");
    }

    let mut base = if digits.starts_with(b"0") { 8 } else { 10 };
    if let [b'0', second, tail @ ..] = digits
        && !second.is_ascii_digit()
        && multiplier(*second).is_none()
    {
        base = match second.to_ascii_lowercase() {
            b'x' => 16,
            b'o' => 8,
            b'b' => 2,
            _ => {
                let warning = if *second == 0 {
                    "Invalid prefix \"0".to_owned()
                } else {
                    let byte = [*second];
                    let byte = String::from_utf8_lossy(&byte);
                    format!(
                        "Invalid prefix \"0{byte}\", interpreting as \"0\" for backwards compatibility"
                    )
                };
                return (0, Some(warning));
            }
        };
        if tail.is_empty()
            || tail
                .first()
                .is_some_and(|b| whitespace(b) || matches!(b, b'+' | b'-'))
            || tail.get(0..2).is_some_and(|prefix| {
                prefix[0] == b'0'
                    && match prefix[1].to_ascii_lowercase() {
                        b'x' | b'o' => true,
                        b'b' => base != 16,
                        _ => false,
                    }
            })
        {
            return no_digits(input, "no digits after base prefix");
        }
        digits = tail;
    }

    let mut count = 0;
    let mut value = 0_u64;
    let mut overflow = false;
    for &byte in digits {
        let Some(digit) = char::from(byte).to_digit(base) else {
            break;
        };
        count += 1;
        match value
            .checked_mul(u64::from(base))
            .and_then(|n| n.checked_add(u64::from(digit)))
        {
            Some(next) => value = next,
            None => {
                value = u64::MAX;
                overflow = true;
            }
        }
    }
    if count == 0 {
        return no_digits(input, "no valid leading digits");
    }
    let signed_max = (i64::MAX as u64) + u64::from(negative);
    overflow |= value > signed_max;
    let mut result = value as i64;
    if negative && !overflow {
        result = result.wrapping_neg();
    }
    let suffix = &digits[count..];
    let suffix = &suffix[suffix.iter().take_while(|byte| whitespace(byte)).count()..];
    if let Some(&last) = suffix.last() {
        let interpreted_end = leading + text.len() - suffix.len();
        let Some(factor) = multiplier(last) else {
            let input_text = escaped(input);
            let last = escaped(&[last]);
            let interpreted = escaped(&input[..interpreted_end]);
            return (
                result,
                Some(format!(
                    "Invalid quantity \"{input_text}\": unknown multiplier \"{last}\", interpreting as \"{interpreted}\" for backwards compatibility"
                )),
            );
        };
        overflow |= result.checked_mul(factor).is_none();
        result = result.wrapping_mul(factor);
        if suffix.len() > 1 {
            let input_text = escaped(input);
            let interpreted = escaped(&input[..interpreted_end]);
            return (
                result,
                Some(format!(
                    "Invalid quantity \"{input_text}\", interpreting as \"{interpreted}{}\" for backwards compatibility",
                    char::from(last)
                )),
            );
        }
    }
    let warning = overflow.then(|| {
        let input = escaped(input);
        format!("Invalid quantity \"{input}\": value is out of range, using overflow result for backwards compatibility")
    });
    (result, warning)
}
