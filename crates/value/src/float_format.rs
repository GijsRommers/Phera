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
    digits_from_scientific(&text, value.is_sign_negative())
}

fn digits_from_scientific(text: &NumberText, negative: bool) -> Digits {
    let (mantissa, exponent) = text.as_str().split_once('e').expect("scientific notation");
    let mut bytes = [0; 17];
    let mut len = 0;
    for (slot, digit) in bytes
        .iter_mut()
        .zip(mantissa.bytes().filter(|&byte| byte != b'.'))
    {
        *slot = digit;
        len += 1;
    }
    Digits {
        bytes,
        len,
        decpt: exponent.parse::<i32>().expect("decimal exponent") + 1,
        negative,
    }
}

/// Shortest round-tripping digits (dtoa mode 0). At a tie PHP uses the
/// ties-even decimal if it fits; a power of two has a narrower interval
/// toward zero and can need the other decimal at the same precision.
fn shortest_digits(value: f64) -> Digits {
    let text = NumberText::formatted(format_args!("{:e}", value.abs()));
    let shortest = digits_from_scientific(&text, value.is_sign_negative());
    let rounded = scientific_digits(value, shortest.len);
    if (shortest.bytes == rounded.bytes && shortest.decpt == rounded.decpt)
        || gcvt_layout(&rounded, 17).as_str().parse::<f64>() == Ok(value)
    {
        rounded
    } else {
        shortest
    }
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

/// The decimal format requested by PHP's `serialize_precision` INI setting.
pub fn float_repr_with_precision(value: f64, precision: i32) -> String {
    with_float_repr(value, precision, str::to_owned)
}

/// Write a float without allocating a temporary string for ordinary precisions.
pub fn with_float_repr<T>(value: f64, precision: i32, write: impl FnOnce(&str) -> T) -> T {
    if precision < 0 || !value.is_finite() {
        return write(gcvt(value, -1).as_str());
    }
    if (1..=17).contains(&precision) {
        return write(gcvt(value, precision).as_str());
    }
    if precision == 0 {
        return write(gcvt_layout(&rounded_digits(value, 1), 0).as_str());
    }
    write(&float_repr_high_precision(value, precision))
}

fn float_repr_high_precision(value: f64, precision: i32) -> String {
    // A binary64 has at most 767 significant decimal digits in its exact expansion.
    let places = (precision - 1).min(767) as usize;
    let scientific = format!("{:.places$e}", value.abs());
    let Some((coefficient, exponent)) = scientific.split_once('e') else {
        return float_repr(value);
    };
    let point = exponent.parse::<i32>().unwrap_or(0) + 1;
    let mut digits = coefficient.replace('.', "");
    while digits.len() > 1 && digits.ends_with('0') {
        digits.pop();
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    if point > precision || point < -3 {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        return format!("{sign}{first}.{rest}E{:+}", point - 1);
    }
    if point <= 0 {
        return format!("{sign}0.{}{digits}", "0".repeat((-point) as usize));
    }
    let point = point as usize;
    if point >= digits.len() {
        return format!("{sign}{digits}{}", "0".repeat(point - digits.len()));
    }
    let (whole, fraction) = digits.split_at(point);
    format!("{sign}{whole}.{fraction}")
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
