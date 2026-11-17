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

/// Decimal digits and point position, as `zend_dtoa` would report them:
/// the value is `0.digits * 10^decpt` (with the first digit non-zero), i.e.
/// `decpt` is one plus the 10-exponent.
struct Digits {
    digits: Vec<u8>,
    decpt: i32,
    negative: bool,
}

/// Shortest round-tripping digits (dtoa mode 0). Rust's LowerExp is a
/// shortest round-trip form too, but where two same-length candidates
/// round-trip (a decimal tie, e.g. 1000000000000000.25) Rust and David
/// Gay's dtoa can pick different ones, so grow the digit count until the
/// *ties-even rounded* form reads back to the same double.
fn shortest_digits(v: f64) -> Digits {
    let neg = v.is_sign_negative();
    for k in 1..=17u32 {
        let nd = (k - 1) as usize;
        let s = format!("{:.nd$e}", v.abs());
        // s is like "3.0000000000000004e-1"
        let (mantissa, exp) = s.split_once('e').unwrap();
        let exp: i32 = exp.parse().unwrap();
        let digits: Vec<u8> = mantissa.bytes().filter(|b| *b != b'.').collect();
        let d = Digits {
            digits,
            decpt: exp + 1,
            negative: neg,
        };
        // Round-trip check via the fully formatted string.
        let text = gcvt_layout(&d, 17);
        if text.parse::<f64>() == Ok(v) {
            return d;
        }
    }
    unreachable!("17 digits always round-trip for finite doubles");
}

/// Digits rounded to `ndigit` significant digits with trailing zeros
/// stripped (dtoa mode 2). Rust's exact LowerExp formatting is correctly
/// rounded, like `zend_dtoa`.
fn rounded_digits(v: f64, ndigit: u32) -> Digits {
    let neg = v.is_sign_negative();
    let s = format!("{:.*e}", ndigit as usize - 1, v.abs());
    let (mantissa, exp) = s.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let digits: Vec<u8> = mantissa.bytes().filter(|b| *b != b'.').collect();
    // Strip trailing zeros; keep at least one digit, as dtoa does.
    let mut digits = digits;
    while digits.len() > 1 && *digits.last().unwrap() == b'0' {
        digits.pop();
    }
    Digits {
        digits,
        decpt: exp + 1,
        negative: neg,
    }
}

/// Port of `zend_gcvt`'s layout logic (buf writing aside).
fn gcvt_layout(d: &Digits, ndigit: i32) -> String {
    let mut out = String::with_capacity(d.digits.len() + 8);
    if d.negative {
        out.push('-');
    }
    let digits = &d.digits;
    let decpt = d.decpt;
    if (decpt >= 0 && decpt > ndigit) || decpt < -3 {
        // Exponential: one digit, point, rest, E, sign, exponent (no pads).
        out.push(digits[0] as char);
        out.push('.');
        if digits.len() == 1 {
            out.push('0');
        } else {
            out.extend(digits[1..].iter().map(|&b| b as char));
        }
        out.push('E');
        let mut exp = decpt - 1;
        if exp < 0 {
            out.push('-');
            exp = -exp;
        } else {
            out.push('+');
        }
        out.push_str(&exp.to_string());
    } else if decpt < 0 {
        // 0.000ddd
        out.push('0');
        out.push('.');
        for _ in 0..-decpt {
            out.push('0');
        }
        out.extend(digits.iter().map(|&b| b as char));
    } else {
        // ddd.ddd with zero padding beyond the digits.
        for i in 0..decpt as usize {
            out.push(if i < digits.len() {
                digits[i] as char
            } else {
                '0'
            });
        }
        if digits.len() as i64 > decpt as i64 {
            if decpt == 0 {
                out.push('0'); // zero before the decimal point
            }
            out.push('.');
            out.extend(digits[decpt as usize..].iter().map(|&b| b as char));
        }
    }
    out
}

/// Shared pre-checks of `zend_gcvt`: infinity and NaN.
fn gcvt(value: f64, ndigit: i32) -> String {
    if value.is_nan() {
        return "NAN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 {
            "INF".to_string()
        } else {
            "-INF".to_string()
        };
    }
    let d = if ndigit >= 0 {
        rounded_digits(value, ndigit as u32)
    } else {
        shortest_digits(value)
    };
    // For layout, mode 0 uses ndigit = 17.
    gcvt_layout(&d, if ndigit >= 0 { ndigit } else { 17 })
}

/// String conversion with PHP's default `precision = 14` (echo, casts,
/// concatenation). `0.1 + 0.2` gives `0.3`, `1.0` gives `1`,
/// `1e25` gives `1.0E+25`, `-0.0` gives `-0`.
pub fn float_to_string_precision14(v: f64) -> String {
    gcvt(v, 14)
}

/// Shortest round-tripping representation (PHP's `serialize_precision = -1`:
/// var_dump, var_export, json_encode). `0.1 + 0.2` gives
/// `0.30000000000000004`, `1.0` gives `1`, `1e25` gives `1.0E+25`.
pub fn float_repr(v: f64) -> String {
    gcvt(v, -1)
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
