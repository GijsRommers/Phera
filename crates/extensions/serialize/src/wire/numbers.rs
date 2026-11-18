//! `unserialize('i:+0005;')` returns 5, while a string length has unsigned syntax.

pub(super) fn digits_end(input: &[u8], start: usize) -> usize {
    let length = input
        .get(start..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    start + length
}

pub(super) fn unsigned(digits: &[u8]) -> usize {
    digits.iter().fold(0usize, |value, digit| {
        value.wrapping_mul(10).wrapping_add((digit - b'0') as usize)
    })
}

/// Consume an ordinary unsigned field and its delimiter together. Long and
/// signed fields keep the caller's PHP-specific overflow and offset behavior.
pub(super) fn short_unsigned(bytes: &[u8], delimiter: &[u8]) -> Option<(usize, usize)> {
    let mut value = 0usize;
    for (index, &byte) in bytes.iter().take(19).enumerate() {
        if byte.is_ascii_digit() {
            value = value
                .wrapping_mul(10)
                .wrapping_add(usize::from(byte - b'0'));
        } else if index != 0 && bytes[index..].starts_with(delimiter) {
            return Some((value, index + delimiter.len()));
        } else {
            return None;
        }
    }
    None
}

pub(super) fn signed(digits: &[u8]) -> (i64, bool) {
    let (negative, digits) = match digits {
        [b'-', digits @ ..] => (true, digits),
        [b'+', digits @ ..] => (false, digits),
        _ => (false, digits),
    };
    let bound = if negative {
        1u64 << 63
    } else {
        i64::MAX as u64
    };
    let mut value = 0u64;
    for digit in digits {
        let digit = (digit - b'0') as u64;
        if value > (bound - digit) / 10 {
            return (if negative { i64::MIN } else { i64::MAX }, true);
        }
        value = value * 10 + digit;
    }
    (
        if negative {
            (value as i64).wrapping_neg()
        } else {
            value as i64
        },
        false,
    )
}

pub(super) fn signed_end(input: &[u8], start: usize) -> (usize, bool) {
    let digits = start + usize::from(matches!(input.get(start), Some(b'+' | b'-')));
    let end = digits_end(input, digits);
    (end, end > digits)
}

pub(super) fn float_end(input: &[u8], start: usize) -> Option<usize> {
    let (mut end, mut any_digits) = signed_end(input, start);
    if input.get(end) == Some(&b'.') {
        end += 1;
        let fraction_end = digits_end(input, end);
        any_digits |= fraction_end != end;
        end = fraction_end;
    }
    if !any_digits {
        return None;
    }
    if matches!(input.get(end), Some(b'e' | b'E')) {
        let (exponent_end, has_exponent) = signed_end(input, end + 1);
        if !has_exponent {
            return None;
        }
        end = exponent_end;
    }
    Some(end)
}

/// Single correctly rounded division suffices for decimal coefficients and
/// divisors that are both representable exactly as binary64.
pub(super) fn decimal(bytes: &[u8]) -> Option<(f64, usize)> {
    let (sign, prefix) = match bytes.first() {
        Some(b'-') => (-1.0, 1),
        Some(b'+') => (1.0, 1),
        _ => (1.0, 0),
    };
    let mut coefficient = 0u64;
    let mut divider = 0u64;
    let mut digits = false;
    for (index, &digit) in bytes[prefix..].iter().take(16).enumerate() {
        match digit {
            b'0'..=b'9' => {
                digits = true;
                coefficient = coefficient * 10 + u64::from(digit - b'0');
                divider *= 10;
            }
            b'.' if divider == 0 => divider = 1,
            b';' if digits => {
                let value = coefficient as f64 / divider.max(1) as f64 * sign;
                return Some((value, prefix + index));
            }
            _ => return None,
        }
    }
    None
}
