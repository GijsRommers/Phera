//! Integers written as decimal text without a formatter.
//!
//! ```php
//! $text = (string) $id;     // "12345"
//! $key = 'user.' . $id;     // "user.12345"
//! $ids = implode(' ', $ids);
//! ```
//!
//! The digits go two at a time into a 20-byte array, which holds every `i64`,
//! and from there into the string or buffer that wants them. Nothing here
//! depends on how a [`PhpString`] keeps its bytes.

use crate::string::PhpString;

/// Every number from 00 to 99 as two digits, for writing an integer two at a time.
const PAIRS: &[u8; 200] = b"0001020304050607080910111213141516171819\
2021222324252627282930313233343536373839\
4041424344454647484950515253545556575859\
6061626364656667686970717273747576777879\
8081828384858687888990919293949596979899";

/// The decimal digits of `number` at the end of the returned array, and
/// where they start.
#[inline]
fn integer_digits(number: i64) -> ([u8; 20], usize) {
    let mut digits = [0u8; 20];
    let mut at = digits.len();
    let mut rest = number.unsigned_abs();
    while rest >= 100 {
        let pair = (rest % 100) as usize * 2;
        rest /= 100;
        at -= 2;
        digits[at] = PAIRS[pair];
        digits[at + 1] = PAIRS[pair + 1];
    }
    if rest >= 10 {
        let pair = rest as usize * 2;
        at -= 2;
        digits[at] = PAIRS[pair];
        digits[at + 1] = PAIRS[pair + 1];
    } else {
        at -= 1;
        digits[at] = b'0' + rest as u8;
    }
    if number < 0 {
        at -= 1;
        digits[at] = b'-';
    }
    (digits, at)
}

/// Appends `number` in decimal to `out`, as `(string) $number` writes it,
/// without going through a formatter.
pub fn push_integer(out: &mut Vec<u8>, number: i64) {
    let (digits, at) = integer_digits(number);
    out.extend_from_slice(&digits[at..]);
}

/// `(string) $number` for an integer, in one buffer of its length.
pub fn integer_string(number: i64) -> PhpString {
    let (digits, at) = integer_digits(number);
    PhpString::from(&digits[at..])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every integer reads as `i64::to_string` writes it, at the edges too.
    #[test]
    fn integers_are_written_as_the_formatter_writes_them() {
        for number in [
            0,
            1,
            9,
            10,
            99,
            100,
            12_345,
            -1,
            -10,
            -987_654_321,
            i64::MAX,
            i64::MIN,
            i64::MIN + 1,
            1_000_000_000_000,
        ]
        .into_iter()
        .chain(-2_500..2_500)
        .chain((0..19).flat_map(|power| {
            let ten = 10_i64.pow(power);
            [ten - 1, ten, ten + 1, -ten]
        })) {
            assert_eq!(
                integer_string(number).as_bytes(),
                number.to_string().as_bytes()
            );
            let mut out = b"x".to_vec();
            push_integer(&mut out, number);
            assert_eq!(out, format!("x{number}").into_bytes());
        }
    }
}
