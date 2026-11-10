//! Concatenation (`.`).

use crate::error::{Diagnostics, OpError};
use crate::string::PhpString;
use crate::value::Value;

/// `concat_function`: both sides through string conversion; arrays give
/// `"Array"` with a warning, objects without `__toString` throw (returned
/// here as an OpError).
pub fn concat(a: &Value, b: &Value, d: &mut dyn Diagnostics) -> Result<Value, OpError> {
    let sa = crate::convert::to_php_string(a, d)?;
    let sb = crate::convert::to_php_string(b, d)?;
    let mut out = Vec::with_capacity(sa.len() + sb.len());
    out.extend_from_slice(&sa);
    out.extend_from_slice(&sb);
    Ok(Value::String(PhpString::from(out)))
}
