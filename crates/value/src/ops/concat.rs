//! Concatenation (`.`).

use crate::error::{Diagnostics, OpError};
use crate::string::PhpString;
use crate::value::Value;

/// `concat_function`: both sides through string conversion; arrays give
/// `"Array"` with a warning, objects without `__toString` throw (returned
/// here as an OpError).
pub fn concat(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    if let (Some(sa), Some(sb)) = (a.as_str_bytes(), b.as_str_bytes()) {
        if sa.is_empty() {
            return Ok(b.clone());
        }
        if sb.is_empty() {
            return Ok(a.clone());
        }
        let mut out = Vec::with_capacity(sa.len() + sb.len());
        out.extend_from_slice(sa);
        out.extend_from_slice(sb);
        return Ok(Value::String(PhpString::from(out)));
    }
    let a_val = crate::convert::to_string_value(a, d)?;
    let b_val = crate::convert::to_string_value(b, d)?;
    let sa = a_val.as_str_bytes().expect("string value");
    let sb = b_val.as_str_bytes().expect("string value");
    if sa.is_empty() {
        return Ok(b_val);
    }
    if sb.is_empty() {
        return Ok(a_val);
    }
    let mut out = Vec::with_capacity(sa.len() + sb.len());
    out.extend_from_slice(sa);
    out.extend_from_slice(sb);
    Ok(Value::String(PhpString::from(out)))
}
