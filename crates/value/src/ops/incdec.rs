//! `++` and `--`, including PHP's Perl-style string increment and the
//! PHP 8.3 deprecations around non-numeric strings.

use crate::convert::{NumVal, parse_numeric};
use crate::error::{Diagnostics, OpError};
use crate::ops::bitwise::value_name;
use crate::ops::deref_clone;
use crate::string::PhpString;
use crate::value::Value;

/// `$v++`.
pub fn increment(v: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let v = deref_clone(v);
    match &v {
        Value::Int(i) => Ok(match i.checked_add(1) {
            Some(x) => Value::Int(x),
            None => Value::Float(*i as f64 + 1.0),
        }),
        Value::Float(f) => Ok(Value::Float(f + 1.0)),
        Value::Null => Ok(Value::Int(1)),
        Value::Bool(_) => {
            d.warning(
                "Increment on type bool has no effect, this will change in the next major version of PHP"
                    .to_string(),
            );
            Ok(v)
        }
        Value::InternedStr(_) | Value::String(_) => {
            let bytes = v.as_str_bytes().expect("checked string");
            let p = parse_numeric(bytes);
            match p.value {
                // Strictly numeric only (is_numeric_str_function): trailing
                // data falls through to the string increment.
                Some(NumVal::Int(i)) if p.exact => Ok(match i.checked_add(1) {
                    Some(x) => Value::Int(x),
                    None => Value::Float(i as f64 + 1.0),
                }),
                Some(NumVal::Float(f)) if p.exact => Ok(Value::Float(f + 1.0)),
                _ => {
                    d.deprecated(
                        "Increment on non-numeric string is deprecated, use str_increment() instead"
                            .to_string(),
                    );
                    Ok(Value::String(php_string_increment(bytes)))
                }
            }
        }
        _ => Err(OpError::type_error(format!(
            "Cannot increment {}",
            value_name(&v)
        ))),
    }
}

/// `$v--`.
pub fn decrement(v: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let v = deref_clone(v);
    match &v {
        Value::Int(i) => Ok(match i.checked_sub(1) {
            Some(x) => Value::Int(x),
            None => Value::Float(*i as f64 - 1.0),
        }),
        Value::Float(f) => Ok(Value::Float(f - 1.0)),
        Value::Null => {
            d.warning(
                "Decrement on type null has no effect, this will change in the next major version of PHP"
                    .to_string(),
            );
            Ok(v)
        }
        Value::Bool(_) => {
            d.warning(
                "Decrement on type bool has no effect, this will change in the next major version of PHP"
                    .to_string(),
            );
            Ok(v)
        }
        Value::InternedStr(_) | Value::String(_) => {
            let bytes = v.as_str_bytes().expect("checked string");
            let p = parse_numeric(bytes);
            match p.value {
                Some(NumVal::Int(i)) if p.exact => Ok(match i.checked_sub(1) {
                    Some(x) => Value::Int(x),
                    None => Value::Float(i as f64 - 1.0),
                }),
                Some(NumVal::Float(f)) if p.exact => Ok(Value::Float(f - 1.0)),
                _ if bytes.is_empty() => {
                    d.deprecated(
                        "Decrement on empty string is deprecated as non-numeric".to_string(),
                    );
                    Ok(Value::Int(-1))
                }
                _ => {
                    // Decrement never does string arithmetic in PHP: the
                    // value is left untouched.
                    d.deprecated(
                        "Decrement on non-numeric string has no effect and is deprecated"
                            .to_string(),
                    );
                    Ok(v)
                }
            }
        }
        _ => Err(OpError::type_error(format!(
            "Cannot decrement {}",
            value_name(&v)
        ))),
    }
}

/// Perl-style string increment (`increment_string`): walk from the end,
/// carrying through `z`→`a`/`Z`→`A`/`9`→`0`, then prepend `a`/`A`/`1` when
/// everything carried. Runs of other bytes stop the walk.
fn php_string_increment(bytes: &[u8]) -> PhpString {
    let mut bytes = bytes.to_vec();
    if bytes.is_empty() {
        return PhpString::from("1");
    }
    const NUMERIC: u8 = 1;
    const UPPER: u8 = 2;
    const LOWER: u8 = 3;
    let mut last = NUMERIC;
    #[allow(unused_assignments)]
    let mut carry = 0u8;
    let mut pos = bytes.len() - 1;
    loop {
        let ch = bytes[pos];
        match ch {
            b'a'..=b'z' => {
                if ch == b'z' {
                    bytes[pos] = b'a';
                    carry = 1;
                } else {
                    bytes[pos] += 1;
                    carry = 0;
                }
                last = LOWER;
            }
            b'A'..=b'Z' => {
                if ch == b'Z' {
                    bytes[pos] = b'A';
                    carry = 1;
                } else {
                    bytes[pos] += 1;
                    carry = 0;
                }
                last = UPPER;
            }
            b'0'..=b'9' => {
                if ch == b'9' {
                    bytes[pos] = b'0';
                    carry = 1;
                } else {
                    bytes[pos] += 1;
                    carry = 0;
                }
                last = NUMERIC;
            }
            _ => {
                carry = 0;
                break;
            }
        }
        if carry == 0 || pos == 0 {
            break;
        }
        pos -= 1;
    }
    if carry == 1 {
        let prefix = match last {
            NUMERIC => b'1',
            UPPER => b'A',
            _ => b'a',
        };
        bytes.insert(0, prefix);
    }
    PhpString::from(bytes)
}
