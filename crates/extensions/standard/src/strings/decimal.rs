//! The decimal digits of a double for `%f`, `%e`, `%g` and `number_format`,
//! following php-src Zend/zend_strtod.c (zend_dtoa)
//! and main/snprintf.c (php_conv_fp and php_gcvt), using decimal integer arithmetic.

#[derive(Clone, Copy, PartialEq, Eq)]
enum Notation {
    General,
    Scientific,
}

/// Decimal digits and the number of digits preceding the decimal point.
/// Integer arithmetic avoids introducing another binary rounding before dtoa's
/// round-to-nearest, ties-to-even step.
pub(super) fn digits(number: f64) -> (Vec<u8>, i32) {
    let bits = number.abs().to_bits();
    let exponent = ((bits >> 52) & 2047) as i32;
    let mantissa = (bits & ((1u64 << 52) - 1)) | if exponent == 0 { 0 } else { 1u64 << 52 };
    let power = if exponent == 0 {
        -1074
    } else {
        exponent - 1075
    };
    let mut digits: Vec<u8> = mantissa
        .to_string()
        .bytes()
        .rev()
        .map(|byte| byte - b'0')
        .collect();
    let factor = if power < 0 { 5 } else { 2 };
    for _ in 0..power.unsigned_abs() {
        let mut carry = 0u16;
        for digit in &mut digits {
            let product = u16::from(*digit) * factor + carry;
            *digit = (product % 10) as u8;
            carry = product / 10;
        }
        while carry > 0 {
            digits.push((carry % 10) as u8);
            carry /= 10;
        }
    }
    digits.reverse();
    let point = digits.len() as i32 + power.min(0);
    (digits, point)
}

/// Discard decimal digits only once; an exact halfway value rounds to even.
fn rounded(mut digits: Vec<u8>, keep: i32, mut point: i32) -> (Vec<u8>, i32) {
    if keep < 0 {
        return (vec![0], 1);
    }
    let keep = keep as usize;
    let next = digits.get(keep).copied().unwrap_or(0);
    let odd = keep
        .checked_sub(1)
        .and_then(|index| digits.get(index))
        .is_some_and(|digit| digit % 2 == 1);
    let up =
        next > 5 || (next == 5 && (odd || digits.iter().skip(keep + 1).any(|digit| *digit != 0)));
    digits.truncate(keep);
    if up {
        let mut carry = true;
        for digit in digits.iter_mut().rev() {
            if *digit == 9 {
                *digit = 0;
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            digits.insert(0, 1);
            point += 1;
        }
    }
    if digits.is_empty() || digits.iter().all(|digit| *digit == 0) {
        return (vec![0], 1);
    }
    (digits, point)
}

/// Fixed formatting suppresses the sign of exact negative zero, like php_fcvt.
pub(super) fn fixed(number: f64, precision: usize) -> Vec<u8> {
    let (digits, point) = digits(number);
    let (digits, point) = rounded(digits, point + precision as i32, point);
    let mut result = Vec::new();
    for index in 0..point.max(1) {
        result.push(if point <= 0 {
            b'0'
        } else {
            b'0' + digits.get(index as usize).copied().unwrap_or(0)
        });
    }
    if precision > 0 {
        result.push(b'.');
    }
    for index in 0..precision {
        let digit_index = point + index as i32;
        result.push(
            b'0' + if digit_index < 0 {
                0
            } else {
                digits.get(digit_index as usize).copied().unwrap_or(0)
            },
        );
    }
    result
}

/// General formatting switches notation at the same decimal exponent as gcvt.
pub(super) fn significant(number: f64, precision: usize, conversion: u8) -> Vec<u8> {
    let notation = if matches!(conversion, b'g' | b'G' | b'h' | b'H') {
        Notation::General
    } else {
        Notation::Scientific
    };
    let count = if notation == Notation::General {
        precision.max(1)
    } else {
        precision + 1
    };
    let (digits, point) = digits(number);
    let (mut digits, point) = rounded(digits, count as i32, point);
    digits.resize(count, 0);
    if notation == Notation::General {
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
    }
    let scientific = notation == Notation::Scientific || point > count as i32 || point < -3;
    let mut result = Vec::new();
    if scientific {
        append_scientific(&mut result, &digits, point, conversion, notation);
    } else {
        for index in 0..point.max(1) {
            result.push(
                b'0' + if point <= 0 {
                    0
                } else {
                    digits.get(index as usize).copied().unwrap_or(0)
                },
            );
        }
        if point < digits.len() as i32 {
            result.push(b'.');
            for index in point..digits.len() as i32 {
                result.push(
                    b'0' + if index < 0 {
                        0
                    } else {
                        digits.get(index as usize).copied().unwrap_or(0)
                    },
                );
            }
        }
    }
    result
}

/// General scientific notation retains a fractional zero when only one digit remains.
fn append_scientific(
    result: &mut Vec<u8>,
    digits: &[u8],
    point: i32,
    conversion: u8,
    notation: Notation,
) {
    result.push(b'0' + digits.first().copied().unwrap_or(0));
    if digits.len() > 1 || notation == Notation::General {
        result.push(b'.');
        if digits.len() == 1 {
            result.push(b'0');
        }
        result.extend(digits.iter().skip(1).map(|digit| b'0' + digit));
    }
    result.push(if conversion.is_ascii_uppercase() {
        b'E'
    } else {
        b'e'
    });
    let exponent = point - 1;
    result.push(if exponent < 0 { b'-' } else { b'+' });
    result.extend(exponent.unsigned_abs().to_string().bytes());
}
