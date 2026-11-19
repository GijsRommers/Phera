//! Conversions between PHP value types, matching PHP 8.5.

use crate::array::{ArrayKey, PhpArray};
use crate::error::{Diagnostics, ErrorKind, OpError};
use crate::float_format::{float_repr, float_repr_with_precision};
use crate::string::PhpString;
use crate::value::Value;

// ---------------------------------------------------------------------------
// numeric string parsing (port of zend's is_numeric_string)
// ---------------------------------------------------------------------------

/// The value of a numeric string: an int or a float.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NumVal {
    Int(i64),
    Float(f64),
}

/// Result of [`parse_numeric`]: `value` is `None` when the string has no
/// numeric prefix at all; `exact` is true when the whole string (modulo
/// surrounding whitespace) was numeric; `oflow` is set (with the number's
/// sign) when an integer literal was too big for `i64` and became a float.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumericParse {
    pub value: Option<NumVal>,
    pub exact: bool,
    pub oflow: i8,
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// PHP 8's numeric-string rules: optional leading whitespace, optional sign,
/// digits; optional single `.` with digits; optional exponent. Trailing
/// whitespace is allowed; other trailing data makes the parse non-exact
/// (still accepted, like PHP's leading-numeric strings).
pub fn parse_numeric(bytes: &[u8]) -> NumericParse {
    match scan_numeric(bytes) {
        None => NumericParse {
            value: None,
            exact: false,
            oflow: 0,
        },
        Some((val, oflow, mut end)) => {
            // Trailing whitespace keeps the parse exact; other trailing
            // data makes it a leading-numeric string (still valid, as with
            // `allow_errors` in zend).
            let mut junk = false;
            while end < bytes.len() {
                if is_space(bytes[end]) {
                    end += 1;
                } else {
                    junk = true;
                    break;
                }
            }
            NumericParse {
                value: Some(val),
                exact: !junk,
                oflow,
            }
        }
    }
}

/// Scans one leading numeric value, returning it, an overflow flag (an
/// integer literal that became a double: -1/0/1) and the offset just after
/// the numeric part (before any trailing whitespace). Port of
/// `_is_numeric_string_ex` with `allow_errors` semantics.
fn scan_numeric(bytes: &[u8]) -> Option<(NumVal, i8, usize)> {
    let mut i = 0;
    while i < bytes.len() && is_space(bytes[i]) {
        i += 1;
    }
    let _start = i;
    let mut neg = false;
    if i < bytes.len() && bytes[i] == b'-' {
        neg = true;
        i += 1;
    } else if i < bytes.len() && bytes[i] == b'+' {
        i += 1;
    }
    if i >= bytes.len() {
        return None;
    }
    let mut oflow: i8 = 0;
    // Token start: just past the sign.
    let tok = i;

    // Extent of the decimal literal starting at `i` (digits, optional
    // `.`+digits, optional exponent); `end` is just past it.
    let extent = |mut j: usize| -> Option<usize> {
        let mut seen_digit = false;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
            seen_digit = true;
        }
        if j < bytes.len() && bytes[j] == b'.' {
            let dot = j;
            j += 1;
            let mut frac = false;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
                frac = true;
            }
            // "digit ." is a valid strtod subject sequence ("5."); a lone
            // dot is only allowed when digits followed it.
            if !(seen_digit || frac) {
                return Some(dot);
            }
            seen_digit = seen_digit || frac;
        }
        if j < bytes.len() && (bytes[j] == b'e' || bytes[j] == b'E') {
            let mut k = j + 1;
            if k < bytes.len() && (bytes[k] == b'-' || bytes[k] == b'+') {
                k += 1;
            }
            if k < bytes.len() && bytes[k].is_ascii_digit() {
                j = k;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    j += 1;
                }
            }
        }
        seen_digit.then_some(j)
    };

    let as_float = |from: usize| -> Option<(NumVal, usize)> {
        let end = extent(from)?;
        let text = std::str::from_utf8(&bytes[from..end]).ok()?;
        let f: f64 = text.parse().ok()?;
        Some((NumVal::Float(if neg { -f } else { f }), end))
    };

    if bytes[i].is_ascii_digit() {
        // Skip leading zeros first, exactly like zend does.
        while i < bytes.len() && bytes[i] == b'0' {
            i += 1;
        }
        let mut tmp: u64 = 0;
        let mut digits = 0;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            tmp = tmp
                .wrapping_mul(10)
                .wrapping_add(u64::from(bytes[i] - b'0'));
            digits += 1;
            i += 1;
            if digits >= 20 {
                // Too many digits for a long: double (tmp's wrap does not
                // matter; the re-parse recomputes the value).
                oflow = if neg { -1 } else { 1 };
                return as_float(tok).map(|(v, e)| (v, oflow, e));
            }
        }
        if i < bytes.len() && (bytes[i] == b'.' || bytes[i] == b'e' || bytes[i] == b'E') {
            return as_float(tok).map(|(v, e)| (v, oflow, e));
        }
        // Integer candidate: exactly 19 digits must still fit in i64.
        if digits == 19 {
            let s = &bytes[i - 19..i];
            let bound = b"9223372036854775808".as_slice();
            if s > bound || (s == bound && !neg) {
                oflow = if neg { -1 } else { 1 };
                return as_float(tok).map(|(v, e)| (v, oflow, e));
            }
        }
        let v = if neg {
            (tmp as i64).wrapping_neg()
        } else {
            tmp as i64
        };
        Some((NumVal::Int(v), 0, i))
    } else if bytes[i] == b'.' {
        as_float(tok).map(|(v, e)| (v, 0, e))
    } else {
        None
    }
}

/// `is_numeric()`: whole string (with surrounding whitespace) numeric.
pub fn is_numeric(bytes: &[u8]) -> bool {
    let p = parse_numeric(bytes);
    p.value.is_some() && p.exact
}

/// `zend_strtod(string)` semantics as used by the explicit `(float)` cast:
/// leading whitespace and sign, then a decimal literal; trailing data is
/// ignored. No inf/NaN words and no hex; a failed scan yields 0.0. Unlike
/// `is_numeric` floats, `-0` keeps its sign.
pub fn strtod_prefix(bytes: &[u8]) -> f64 {
    let mut i = 0;
    while i < bytes.len() && is_space(bytes[i]) {
        i += 1;
    }
    let tok = i;
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
        i += 1;
    }
    let ok_start = i < bytes.len()
        && (bytes[i].is_ascii_digit()
            || (bytes[i] == b'.' && bytes.get(i + 1).is_some_and(|b| b.is_ascii_digit())));
    if !ok_start {
        return 0.0;
    }
    // Same literal extent rules as the numeric scanner, but parsed with the
    // sign included so that "-0" keeps its sign.
    let mut j = i;
    let mut seen_digit = false;
    while j < bytes.len() && bytes[j].is_ascii_digit() {
        j += 1;
        seen_digit = true;
    }
    if j < bytes.len() && bytes[j] == b'.' {
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
            seen_digit = true;
        }
    }
    if !seen_digit {
        return 0.0;
    }
    if j < bytes.len() && (bytes[j] == b'e' || bytes[j] == b'E') {
        let mut k = j + 1;
        if k < bytes.len() && (bytes[k] == b'-' || bytes[k] == b'+') {
            k += 1;
        }
        if k < bytes.len() && bytes[k].is_ascii_digit() {
            j = k;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
        }
    }
    std::str::from_utf8(&bytes[tok..j])
        .ok()
        .and_then(|t| t.parse::<f64>().ok())
        .unwrap_or(0.0)
}

// ---------------------------------------------------------------------------
// float -> int
// ---------------------------------------------------------------------------

const TWO_POW_63: f64 = 9223372036854775808.0;

/// `zend_dval_to_lval`: float to int cast semantics. Non-finite or
/// out-of-range doubles produce a warning; out-of-range results wrap
/// modulo 2^64 like `zend_dval_to_lval_slow` does on 64-bit.
pub fn float_to_int_cast(d: f64, diag: &mut dyn Diagnostics) -> i64 {
    if !d.is_finite() || !fits_i64(d) {
        diag.warning(format!(
            "The float {} is not representable as an int, cast occurred",
            float_repr(d)
        ));
    }
    if !d.is_finite() {
        return 0;
    }
    if !fits_i64(d) {
        return dval_to_lval_slow(d);
    }
    d as i64
}

/// `zend_dval_to_lval_safe`, used for array keys: also deprecates when the
/// truncation itself loses precision.
pub fn float_to_key_int(d: f64, diag: &mut dyn Diagnostics) -> i64 {
    let l = float_to_int_cast(d, diag);
    if fits_i64(d) && l as f64 != d {
        diag.deprecated(format!(
            "Implicit conversion from float {} to int loses precision",
            float_repr(d)
        ));
    }
    l
}

/// `ZEND_DOUBLE_FITS_LONG`: note `(double)i64::MAX` rounds to 2^63, so the
/// upper bound is exclusive.
fn fits_i64(d: f64) -> bool {
    !(d >= TWO_POW_63 || d < -TWO_POW_63)
}

fn dval_to_lval_slow(d: f64) -> i64 {
    let two64 = 18446744073709551616.0;
    let mut dmod = d % two64;
    if dmod < 0.0 {
        dmod += two64;
    }
    (dmod as u64) as i64
}

/// String-float to int inside `(int)` casts (`zend_dval_to_lval_cap`):
/// silently clamps instead of wrapping, and gives 0 for non-finite values.
fn float_string_to_int(d: f64) -> i64 {
    if !d.is_finite() {
        0
    } else if !fits_i64(d) {
        if d > 0.0 { i64::MAX } else { i64::MIN }
    } else {
        d as i64
    }
}

// ---------------------------------------------------------------------------
// conversions
// ---------------------------------------------------------------------------

/// Truthiness for comparisons and operators (`zend_is_true`): no warnings.
pub fn to_bool(v: &Value) -> bool {
    match &*v.deref() {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(i) => *i != 0,
        Value::Float(f) => *f != 0.0, // NaN is true, -0.0 is false
        Value::String(s) => !(s.is_empty() || (s.len() == 1 && s[0] == b'0')),
        Value::Array(a) => !a.is_empty(),
        Value::Object(_) => true,
        Value::Resource(_) => true,
        Value::Reference(_) => unreachable!("deref returns non-reference values"),
    }
}

/// `(bool)` cast: warns when a NaN is coerced, like PHP's
/// `convert_to_bool`.
pub fn cast_to_bool(v: &Value, diag: &mut dyn Diagnostics) -> bool {
    if let Value::Float(f) = &*v.deref()
        && f.is_nan()
    {
        diag.warning("unexpected NAN value was coerced to bool".to_string());
    }
    to_bool(v)
}

/// `(int)` cast. Warnings exactly like PHP 8.5 (`The float ... is not
/// representable as an int, cast occurred` for out-of-range doubles; strings
/// clamp silently).
pub fn to_int(v: &Value, diag: &mut dyn Diagnostics) -> Result<i64, OpError> {
    match &*v.deref() {
        Value::Null => Ok(0),
        Value::Bool(b) => Ok(*b as i64),
        Value::Int(i) => Ok(*i),
        Value::Float(f) => Ok(float_to_int_cast(*f, diag)),
        Value::String(s) => Ok(match parse_numeric(s.as_bytes()).value {
            Some(NumVal::Int(i)) => i,
            Some(NumVal::Float(f)) => float_string_to_int(f),
            None => 0,
        }),
        Value::Array(a) => Ok(i64::from(!a.is_empty())),
        // Casting an object warns and gives 1; only arithmetic on one throws.
        Value::Object(o) => {
            diag.warning(format!(
                "Object of class {} could not be converted to int",
                o.class.name()
            ));
            Ok(1)
        }
        Value::Resource(r) => Ok(r.id),
        Value::Reference(_) => unreachable!("deref returns non-reference values"),
    }
}

/// `(float)` cast.
pub fn to_float(v: &Value, diag: &mut dyn Diagnostics) -> Result<f64, OpError> {
    match &*v.deref() {
        Value::Null => Ok(0.0),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        Value::String(s) => Ok(strtod_prefix(s.as_bytes())),
        Value::Array(a) => Ok(if a.is_empty() { 0.0 } else { 1.0 }),
        Value::Object(o) => {
            diag.warning(format!(
                "Object of class {} could not be converted to float",
                o.class.name()
            ));
            Ok(1.0)
        }
        Value::Resource(r) => Ok(r.id as f64),
        Value::Reference(_) => unreachable!("deref returns non-reference values"),
    }
}

/// String conversion (`(string)` cast, concatenation operand). Emits PHP's
/// warnings: `Array to string conversion`, and for NaN
/// `unexpected NAN value was coerced to string`.
pub fn to_php_string(v: &Value, diag: &mut dyn Diagnostics) -> Result<PhpString, OpError> {
    match &*v.deref() {
        Value::Null => Ok(PhpString::new()),
        Value::Bool(b) => Ok(PhpString::from(if *b { "1" } else { "" })),
        Value::Int(i) => Ok(PhpString::from(i.to_string())),
        Value::Float(f) => {
            if f.is_nan() {
                diag.warning("unexpected NAN value was coerced to string".to_string());
            }
            Ok(PhpString::from(float_repr_with_precision(
                *f,
                diag.float_precision(),
            )))
        }
        Value::String(s) => Ok(s.clone()),
        Value::Array(_) => {
            diag.warning("Array to string conversion".to_string());
            Ok(PhpString::from("Array"))
        }
        Value::Object(o) => Err(OpError::new(
            ErrorKind::Error,
            format!(
                "Object of class {} could not be converted to string",
                o.class.name()
            ),
        )),
        Value::Resource(r) => Ok(PhpString::from(format!("Resource id #{}", r.id))),
        Value::Reference(_) => unreachable!("deref returns non-reference values"),
    }
}

/// `(array)` cast. NaN warns, like PHP's `convert_scalar_to_array`.
pub fn to_array(v: &Value, diag: &mut dyn Diagnostics) -> PhpArray {
    if let Value::Float(f) = &*v.deref()
        && f.is_nan()
    {
        diag.warning("unexpected NAN value was coerced to array".to_string());
    }
    match &*v.deref() {
        Value::Null => PhpArray::new(),
        Value::Array(a) => (**a).clone(),
        Value::Object(o) => o.lazy_properties(),
        val => {
            let scalar = match val {
                Value::Bool(b) => Value::Bool(*b),
                Value::Int(i) => Value::Int(*i),
                Value::Float(f) => Value::Float(*f),
                Value::String(s) => Value::String(s.clone()),
                Value::Resource(r) => Value::Resource(r.clone()),
                _ => unreachable!(),
            };
            PhpArray::from_iter([(ArrayKey::Int(0), scalar)])
        }
    }
}
