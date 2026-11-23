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
            return Ok(Value::String(sb.clone()));
        }
        if sb.is_empty() {
            return Ok(Value::String(sa.clone()));
        }
        return Ok(Value::String(PhpString::from_pair(
            sa.as_bytes(),
            sb.as_bytes(),
        )));
    }
    let sa = crate::convert::to_php_string(a, d)?;
    let sb = crate::convert::to_php_string(b, d)?;
    if sa.is_empty() {
        return Ok(Value::String(sb));
    }
    if sb.is_empty() {
        return Ok(Value::String(sa));
    }
    Ok(Value::String(PhpString::from_pair(
        sa.as_bytes(),
        sb.as_bytes(),
    )))
}
