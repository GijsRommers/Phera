//! PHP key normalization, shared by both storage shapes.

use crate::error::{Diagnostics, OpError};
use crate::string::PhpString;
use crate::value::Value;

/// An array key after PHP's normalisation: integer, or a string (used for
/// strings that are not canonical decimal integers in `i64` range).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ArrayKey {
    Int(i64),
    String(PhpString),
}

impl ArrayKey {
    pub fn int(i: i64) -> Self {
        ArrayKey::Int(i)
    }

    pub fn str(s: impl Into<PhpString>) -> Self {
        ArrayKey::String(s.into())
    }

    /// Normalises a PHP value into an array key, reporting deprecations and
    /// warnings to `diag` exactly as PHP 8.5 does.
    ///
    /// * ints stay, floats go through the `(int)` cast (out of range gives 0
    ///   with a warning, precision loss a deprecation);
    /// * `bool` becomes 0/1, `null` becomes `""` with a deprecation;
    /// * strings that are canonical decimal integers in range become ints
    ///   (no leading zeros, at most one leading `-`, no other characters);
    /// * arrays and objects are a TypeError, resources use their id.
    pub fn from_value_diag(value: &Value, diag: &mut dyn Diagnostics) -> Result<ArrayKey, OpError> {
        match &*value.deref() {
            Value::Int(i) => Ok(ArrayKey::Int(*i)),
            Value::Bool(b) => Ok(ArrayKey::Int(*b as i64)),
            Value::Float(f) => Ok(ArrayKey::Int(crate::convert::float_to_key_int(*f, diag))),
            Value::Null => {
                diag.deprecated(
                    "Using null as an array offset is deprecated, use an empty string instead"
                        .to_string(),
                );
                Ok(ArrayKey::str(""))
            }
            Value::String(s) => Ok(string_to_key(s)),
            Value::Array(_) => Err(OpError::type_error(
                "Cannot access offset of type array on array".to_string(),
            )),
            Value::Object(o) => Err(OpError::type_error(format!(
                "Cannot access offset of type {} on array",
                o.class.name()
            ))),
            Value::Resource(r) => {
                diag.warning(format!(
                    "Resource ID#{} used as offset, casting to integer ({})",
                    r.id, r.id
                ));
                Ok(ArrayKey::Int(r.id))
            }
            Value::Reference(_) => unreachable!("deref returns non-reference values"),
        }
    }

    /// Same as [`ArrayKey::from_value_diag`] with diagnostics discarded.
    pub fn from_value(value: &Value) -> Result<ArrayKey, OpError> {
        let mut n = crate::error::NoDiagnostics;
        Self::from_value_diag(value, &mut n)
    }
}

/// `zend_handle_numeric_str_ex`: canonical decimal integer key. Rejects
/// leading zeros (`"05"`, `"-0"`), `+`, whitespace, and anything that does
/// not fit in `i64` (`"9223372036854775808"` stays a string).
///
/// A key that stays a string shares the caller's string (one reference
/// count) instead of copying its bytes: `$row['name']` in a loop would
/// otherwise allocate a new string on every access.
fn string_to_key(string: &PhpString) -> ArrayKey {
    let bytes = string.as_bytes();
    let not_a_key = || ArrayKey::String(string.clone());
    let (digits, neg) = match bytes.split_first() {
        Some((b'-', rest)) => (rest, true),
        _ => (bytes, false),
    };
    // A leading zero (including "-0" and "" after the sign) never converts.
    // `length > 1` in C counts the whole key: "-0" and "00" never convert.
    if digits.is_empty() || (digits[0] == b'0' && bytes.len() > 1) || digits.len() > 19 {
        return not_a_key();
    }
    let mut acc: u64 = 0;
    for &b in digits {
        if !b.is_ascii_digit() {
            return not_a_key();
        }
        acc = acc * 10 + u64::from(b - b'0');
    }
    if neg {
        if acc > i64::MIN.unsigned_abs() {
            return not_a_key();
        }
        ArrayKey::Int((acc as i64).wrapping_neg())
    } else if acc > i64::MAX as u64 {
        not_a_key()
    } else {
        ArrayKey::Int(acc as i64)
    }
}
