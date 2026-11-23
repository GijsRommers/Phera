//! Bitwise operators: `&`, `|`, `^`, `~`, `<<`, `>>`.

use crate::error::{Diagnostics, ErrorKind, OpError};
use crate::ops::{deref_clone, try_get_long2};
use crate::string::PhpString;
use crate::value::Value;

fn strwise(a: &[u8], b: &[u8], f: impl Fn(u8, u8) -> u8, copy_rest: bool) -> PhpString {
    let (longer, shorter) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut out = Vec::with_capacity(if copy_rest {
        longer.len()
    } else {
        shorter.len()
    });
    for i in 0..shorter.len() {
        out.push(f(shorter[i], longer[i]));
    }
    if copy_rest {
        out.extend_from_slice(&longer[shorter.len()..]);
    }
    PhpString::from(out)
}

fn bitwise(
    a: &Value,
    b: &Value,
    sigil: &str,
    copy_rest: bool,
    f: impl Fn(i64, i64) -> i64,
    g: impl Fn(u8, u8) -> u8,
    d: &mut dyn Diagnostics,
) -> Result<Value, OpError> {
    let a = deref_clone(a);
    let b = deref_clone(b);
    if let (Value::Int(x), Value::Int(y)) = (&a, &b) {
        return Ok(Value::Int(f(*x, *y)));
    }
    if let (Some(x), Some(y)) = (a.as_str_bytes(), b.as_str_bytes()) {
        return Ok(Value::String(strwise(x, y, g, copy_rest)));
    }
    let (x, y) = try_get_long2(&a, &b, sigil, d)?;
    Ok(Value::Int(f(x, y)))
}

pub fn bitwise_and(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    bitwise(a, b, "&", false, |x, y| x & y, |x, y| x & y, d)
}

/// `|` keeps the trailing bytes of the longer string (zend copies them).
pub fn bitwise_or(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    bitwise(a, b, "|", true, |x, y| x | y, |x, y| x | y, d)
}

pub fn bitwise_xor(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    bitwise(a, b, "^", false, |x, y| x ^ y, |x, y| x ^ y, d)
}

/// `~`: ints by complement, strings bytewise; other types (including bool
/// and null) are a TypeError.
pub fn bitwise_not(a: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let a = deref_clone(a);
    match &a {
        Value::Int(x) => Ok(Value::Int(!x)),
        Value::Float(f) => {
            let l = crate::convert::float_to_key_int(*f, d);
            Ok(Value::Int(!l))
        }
        Value::InternedStr(_) | Value::String(_) => Ok(Value::String(PhpString::from(
            a.as_str_bytes()
                .expect("checked string")
                .iter()
                .map(|b| !b)
                .collect::<Vec<u8>>(),
        ))),
        _ => Err(OpError::type_error(format!(
            "Cannot perform bitwise not on {}",
            value_name(&a)
        ))),
    }
}

/// `zend_zval_value_name`: the word PHP puts into "Cannot increment X"
/// style messages: `true`/`false` for bools, the class name for objects.
pub(crate) fn value_name(v: &Value) -> String {
    match &v {
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
        Value::Null => "null".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(o) => o.class.name().to_string(),
        Value::Resource(r) => {
            if r.is_closed() {
                "resource (closed)".to_string()
            } else {
                "resource".to_string()
            }
        }
        _ => v.type_name(),
    }
}

fn shift(
    a: &Value,
    b: &Value,
    sigil: &str,
    left: bool,
    d: &mut dyn Diagnostics,
) -> Result<Value, OpError> {
    let a = deref_clone(a);
    let b = deref_clone(b);
    let (x, y) = try_get_long2(&a, &b, sigil, d)?;
    if (y as u64) >= 64 {
        if y < 0 {
            return Err(OpError::new(
                ErrorKind::ArithmeticError,
                "Bit shift by negative number",
            ));
        }
        return Ok(Value::Int(if left {
            0
        } else if x < 0 {
            -1
        } else {
            0
        }));
    }
    Ok(Value::Int(if left {
        (x as u64).wrapping_shl(y as u32) as i64
    } else {
        x.wrapping_shr(y as u32)
    }))
}

pub fn shift_left(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    shift(a, b, "<<", true, d)
}

pub fn shift_right(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    shift(a, b, ">>", false, d)
}
