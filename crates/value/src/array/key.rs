//! Array keys: `$a["5"]` and `$a[5]` are the same entry, `$a["05"]` is not.

use std::hash::{Hash, Hasher};

use crate::error::{Diagnostics, OpError};
use crate::intern::{InternedStr, StrRef};
use crate::string::PhpString;
use crate::value::Value;

/// An array key after PHP's normalisation: integer, or a string (used for
/// strings that are not canonical decimal integers in `i64` range).
#[derive(Clone, Debug)]
pub enum ArrayKey {
    Int(i64),
    Interned(&'static InternedStr),
    String(PhpString),
}

const _: () = assert!(std::mem::size_of::<ArrayKey>() == 16);

impl PartialEq for ArrayKey {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Interned(a), Self::Interned(b)) => std::ptr::eq(*a, *b),
            (Self::Interned(a), Self::String(b)) | (Self::String(b), Self::Interned(a)) => {
                a.as_bytes() == b.as_bytes()
            }
            (Self::String(a), Self::String(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ArrayKey {}

impl Hash for ArrayKey {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::Int(i) => {
                0u8.hash(state);
                i.hash(state);
            }
            Self::Interned(s) => {
                1u8.hash(state);
                s.as_bytes().hash(state);
            }
            Self::String(s) => {
                1u8.hash(state);
                s.as_bytes().hash(state);
            }
        }
    }
}

impl ArrayKey {
    pub fn int(i: i64) -> Self {
        ArrayKey::Int(i)
    }

    pub fn str(s: impl Into<PhpString>) -> Self {
        ArrayKey::String(s.into())
    }

    /// Normalizes a static/compile-time string into an interned array key (or
    /// an integer key if numeric), avoiding any `Rc` allocation or digit scan
    /// on subsequent lookups.
    #[inline]
    pub fn interned(bytes: &[u8]) -> Self {
        match InternedStr::try_intern(bytes) {
            Some(s) => s.to_array_key(),
            None => Self::from_string(PhpString::from(bytes)),
        }
    }

    /// Creates an interned string key without numeric normalisation (for PHP
    /// property names like `"\0Class\0prop"` or declared property keys).
    #[inline]
    pub fn interned_str(bytes: &[u8]) -> Self {
        match InternedStr::try_intern(bytes) {
            Some(s) => ArrayKey::Interned(s),
            None => ArrayKey::String(PhpString::from(bytes)),
        }
    }

    /// Borrows the string bytes if this key is a string key.
    #[inline(always)]
    pub fn as_str_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Int(_) => None,
            Self::Interned(s) => Some(s.as_bytes()),
            Self::String(s) => Some(s.as_bytes()),
        }
    }

    /// Borrows this key as a `StrRef` if it is a string key.
    #[inline(always)]
    pub fn str_ref(&self) -> Option<StrRef<'_>> {
        match self {
            Self::Int(_) => None,
            Self::Interned(s) => Some(StrRef::Interned(s)),
            Self::String(s) => Some(StrRef::Dynamic(s)),
        }
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
                Ok(ArrayKey::Interned(InternedStr::empty()))
            }
            Value::InternedStr(s) => Ok(s.to_array_key()),
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
#[inline]
pub(crate) fn numeric_string_key(bytes: &[u8]) -> Option<i64> {
    let &first = bytes.first()?;
    if !first.is_ascii_digit() && first != b'-' {
        return None;
    }
    numeric_string_key_slow(bytes)
}

#[cold]
#[inline(never)]
fn numeric_string_key_slow(bytes: &[u8]) -> Option<i64> {
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
