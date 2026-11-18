//! Float to string, byte-for-byte compatible with PHP.
//!
//! Both flavours funnel through a port of PHP's `zend_gcvt`
//! (Zend/zend_strtod.c): take decimal digits + decimal point position from
//! `zend_dtoa`, then lay them out — E-notation when `decpt > ndigit` or
//! `decpt < -3`, plain otherwise.
//!
//! * [`float_to_string_precision14`] rounds to `precision ini` = 14
//!   significant digits with trailing zeros stripped (echo, string casts).
//! * [`float_repr`] uses the shortest round-tripping digits
//!   (`serialize_precision` = -1; var_dump, var_export, json_encode).

use std::fmt;
use std::io::{Cursor, Write};

/// A decimal number fits in 32 bytes: at most 17 significant digits,
/// a decimal point, a sign, and a four-character signed exponent after E.
#[derive(Default)]
pub(crate) struct NumberText {
    bytes: [u8; 32],
    len: usize,
}

impl NumberText {
    fn formatted(arguments: fmt::Arguments<'_>) -> Self {
        let mut text = Self::default();
        let mut cursor = Cursor::new(text.bytes.as_mut_slice());
        cursor.write_fmt(arguments).expect("decimal text fits");
        text.len = cursor.position() as usize;
        text
    }

    pub(crate) fn integer(value: i64) -> Self {
        Self::formatted(format_args!("{value}"))
    }

    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len]
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(self.as_bytes()).expect("decimal text is ASCII")
    }

    fn push(&mut self, byte: u8) {
        self.extend(&[byte]);
    }

    fn extend(&mut self, bytes: &[u8]) {
        let end = self.len + bytes.len();
        self.bytes[self.len..end].copy_from_slice(bytes);
        self.len = end;
    }
}

/// Decimal digits and point position, as `zend_dtoa` would report them:
/// the value is `0.digits * 10^decpt` (with the first digit non-zero).
struct Digits {
    bytes: [u8; 17],
    len: usize,
    decpt: i32,
    negative: bool,
}

/// Rust's exact LowerExp formatting rounds as zend_dtoa does.
fn scientific_digits(value: f64, count: usize) -> Digits {
    let places = count - 1;
    let text = NumberText::formatted(format_args!("{:.places$e}", value.abs()));
    let (mantissa, exponent) = text.as_str().split_once('e').expect("scientific notation");
    let mut digits = Digits {
        bytes: [0; 17],
        len: count,
        decpt: exponent.parse::<i32>().expect("decimal exponent") + 1,
        negative: value.is_sign_negative(),
    };
    for (index, digit) in mantissa.bytes().filter(|&byte| byte != b'.').enumerate() {
        digits.bytes[index] = digit;
    }
    digits
}

/// Shortest round-tripping digits (dtoa mode 0). At a decimal tie, Rust
/// and David Gay's dtoa can pick different shortest forms, so grow the
/// digit count until the ties-even rounded form reads back to the input.
fn shortest_digits(value: f64) -> Digits {
    for count in 1..17 {
        let digits = scientific_digits(value, count);
        if gcvt_layout(&digits, 17).as_str().parse::<f64>() == Ok(value) {
            return digits;
        }
    }
    scientific_digits(value, 17)
}

/// Digits rounded to `count` significant digits with trailing zeros
/// stripped (dtoa mode 2).
fn rounded_digits(value: f64, count: usize) -> Digits {
    let mut digits = scientific_digits(value, count);
    while digits.len > 1 && digits.bytes[digits.len - 1] == b'0' {
        digits.len -= 1;
    }
    digits
}

/// Port of `zend_gcvt`'s layout logic.
fn gcvt_layout(d: &Digits, ndigit: i32) -> NumberText {
    let mut out = NumberText::default();
    if d.negative {
        out.push(b'-');
    }
    let digits = &d.bytes[..d.len];
    let decpt = d.decpt;
    if decpt > ndigit || decpt < -3 {
        out.push(digits[0]);
        out.push(b'.');
        if digits.len() == 1 {
            out.push(b'0');
        } else {
            out.extend(&digits[1..]);
        }
        let exponent = NumberText::formatted(format_args!("E{:+}", decpt - 1));
        out.extend(exponent.as_bytes());
    } else if decpt <= 0 {
        out.extend(b"0.");
        for _ in 0..-decpt {
            out.push(b'0');
        }
        out.extend(digits);
    } else {
        let decpt = decpt as usize;
        for i in 0..decpt {
            out.push(digits.get(i).copied().unwrap_or(b'0'));
        }
        if decpt < digits.len() {
            out.push(b'.');
            out.extend(&digits[decpt..]);
        }
    }
    out
}

/// Shared pre-checks of `zend_gcvt` for infinity and NaN.
fn gcvt(value: f64, ndigit: i32) -> NumberText {
    let special: &[u8] = if value.is_nan() {
        b"NAN"
    } else if value == f64::INFINITY {
        b"INF"
    } else if value == f64::NEG_INFINITY {
        b"-INF"
    } else if ndigit >= 0 {
        return gcvt_layout(&rounded_digits(value, ndigit as usize), ndigit);
    } else {
        return gcvt_layout(&shortest_digits(value), 17);
    };
    let mut text = NumberText::default();
    text.extend(special);
    text
}

/// PHP's default float text for loose comparisons with nonnumeric strings.
pub(crate) fn float_text_precision14(value: f64) -> NumberText {
    gcvt(value, 14)
}

/// String conversion with PHP's default `precision = 14` (echo, casts,
/// concatenation). `0.1 + 0.2` gives `0.3`, `1.0` gives `1`,
/// `1e25` gives `1.0E+25`, `-0.0` gives `-0`.
pub fn float_to_string_precision14(v: f64) -> String {
    float_text_precision14(v).as_str().to_owned()
}

/// Shortest round-tripping representation (PHP's `serialize_precision = -1`:
/// var_dump, var_export, json_encode). `0.1 + 0.2` gives
/// `0.30000000000000004`, `1.0` gives `1`, `1e25` gives `1.0E+25`.
pub fn float_repr(v: f64) -> String {
    gcvt(v, -1).as_str().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precision14_matches_php() {
        assert_eq!(float_to_string_precision14(0.1 + 0.2), "0.3");
        assert_eq!(float_to_string_precision14(1.0), "1");
        assert_eq!(float_to_string_precision14(1e25), "1.0E+25");
        assert_eq!(float_to_string_precision14(-0.0), "-0");
        assert_eq!(float_to_string_precision14(1e-5), "1.0E-5");
        assert_eq!(float_to_string_precision14(1e14), "1.0E+14");
        assert_eq!(float_to_string_precision14(1e13), "10000000000000");
        assert_eq!(float_to_string_precision14(1.5e-10), "1.5E-10");
        assert_eq!(float_to_string_precision14(0.0001), "0.0001");
        assert_eq!(float_to_string_precision14(f64::NAN), "NAN");
        assert_eq!(float_to_string_precision14(f64::INFINITY), "INF");
        assert_eq!(float_to_string_precision14(f64::NEG_INFINITY), "-INF");
    }

    #[test]
    fn shortest_matches_php() {
        assert_eq!(float_repr(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(float_repr(1e25), "1.0E+25");
        assert_eq!(float_repr(1.0), "1");
        assert_eq!(float_repr(-0.0), "-0");
        assert_eq!(float_repr(1e-10), "1.0E-10");
        assert_eq!(float_repr(1e14), "100000000000000");
        assert_eq!(float_repr(1e17), "1.0E+17");
        assert_eq!(float_repr(1.0 / 3.0), "0.3333333333333333");
        assert_eq!(float_repr(5e-324), "5.0E-324");
        assert_eq!(float_repr(9.223372036854776e18), "9.223372036854776E+18");
    }
}
