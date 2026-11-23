//! Concatenation (`.`).

use crate::error::{Diagnostics, OpError};
use crate::string::PhpString;
use crate::value::Value;

/// `concat_function`: both sides through string conversion; arrays give
/// `"Array"` with a warning, objects without `__toString` throw (returned
/// here as an OpError).
pub fn concat(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    // An integer is written into the result, not into a string of its own.
    if let (Value::String(left), Value::Int(number)) = (a, b) {
        let mut out = Vec::with_capacity(left.len() + 20);
        out.extend_from_slice(left.as_bytes());
        crate::string::push_integer(&mut out, *number);
        return Ok(Value::String(PhpString::from(out)));
    }
    if let (Value::Int(number), Value::String(right)) = (a, b) {
        let mut out = Vec::with_capacity(right.len() + 20);
        crate::string::push_integer(&mut out, *number);
        out.extend_from_slice(right.as_bytes());
        return Ok(Value::String(PhpString::from(out)));
    }
    let sa = crate::convert::to_php_string(a, d)?;
    let sb = crate::convert::to_php_string(b, d)?;
    let mut out = Vec::with_capacity(sa.len() + sb.len());
    out.extend_from_slice(&sa);
    out.extend_from_slice(&sb);
    Ok(Value::String(PhpString::from(out)))
}
