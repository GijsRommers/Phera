//! Array keys: `$a["5"]` and `$a[5]` are the same entry, `$a["05"]` is not.

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

    /// Normalize an owned string as PHP does for `$array[$key]`.
    pub fn from_string(string: PhpString) -> Self {
        match numeric_string_key(string.as_bytes()) {
            Some(key) => Self::Int(key),
            None => Self::String(string),
        }
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
            Value::String(s) => Ok(match numeric_string_key(s.as_bytes()) {
                Some(key) => Self::Int(key),
                None => Self::String(s.clone()),
            }),
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
fn numeric_string_key(bytes: &[u8]) -> Option<i64> {
    let (digits, neg) = match bytes.split_first() {
        Some((b'-', rest)) => (rest, true),
        _ => (bytes, false),
    };
    // "0" is the only key that may start with 0: not "00", "05" or "-0".
    if digits.is_empty() || (digits[0] == b'0' && bytes.len() > 1) || digits.len() > 19 {
        return None;
    }
    let mut acc: u64 = 0;
    for &b in digits {
        if !b.is_ascii_digit() {
            return None;
        }
        acc = acc * 10 + u64::from(b - b'0');
    }
    if neg {
        if acc > i64::MIN.unsigned_abs() {
            return None;
        }
        Some((acc as i64).wrapping_neg())
    } else if acc > i64::MAX as u64 {
        None
    } else {
        Some(acc as i64)
    }
}
