//! Concatenation (`.`).

use crate::error::{Diagnostics, OpError};
use crate::string::PhpString;
use crate::value::Value;

/// `concat_function`: both sides through string conversion; arrays give
/// `"Array"` with a warning, objects without `__toString` throw (returned
/// here as an OpError).
pub fn concat(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    if let (Value::String(sa), Value::String(sb)) = (a, b) {
        if sa.is_empty() {
            return Ok(b.clone());
        }
        if sb.is_empty() {
            return Ok(a.clone());
        }
        return Ok(Value::String(PhpString::from_pair(
            sa.as_bytes(),
            sb.as_bytes(),
        )));
    }
    let a_val = crate::convert::to_string_value(a, d)?;
    let b_val = crate::convert::to_string_value(b, d)?;
    let (Value::String(sa), Value::String(sb)) = (&a_val, &b_val) else {
        unreachable!("both operands were converted to strings")
    };
    if sa.is_empty() {
        return Ok(b_val);
    }
    if sb.is_empty() {
        return Ok(a_val);
    }
    Ok(Value::String(PhpString::from_pair(
        sa.as_bytes(),
        sb.as_bytes(),
    )))
}
