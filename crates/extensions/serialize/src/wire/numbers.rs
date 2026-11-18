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
