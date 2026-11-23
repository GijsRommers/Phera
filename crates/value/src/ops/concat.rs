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
    Ok(Value::String(PhpString::from_pair(
        sa.as_bytes(),
        sb.as_bytes(),
    )))
}
