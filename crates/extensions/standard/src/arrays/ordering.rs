//! Shared PHP sorting comparisons.
use phera_extension_api::value::{compare, to_float, to_php_string};
use phera_extension_api::{Context, NativeError, Value};
use std::cmp::Ordering;
/// Unknown flags fall back to regular comparison; locale strings use byte order.
pub(super) fn compare_values(
    context: &mut dyn Context,
    left: &Value,
    right: &Value,
    flags: i64,
) -> Result<Ordering, NativeError> {
    match flags & !8 {
        1 => {
            let left = to_float(left, context.diagnostics())?;
            let right = to_float(right, context.diagnostics())?;
            Ok(if left == right {
                Ordering::Equal
            } else if left < right {
                Ordering::Less
            } else {
                Ordering::Greater
            })
        }
        2 | 5 | 6 => {
            let left = to_php_string(left, context.diagnostics())?;
            let right = to_php_string(right, context.diagnostics())?;
            if flags & !8 == 6 {
                return Ok(crate::natural::natural_compare(
                    left.as_bytes(),
                    right.as_bytes(),
                    flags & 8 != 0,
                ));
            }
            if flags & 8 != 0 {
                Ok(left
                    .as_bytes()
                    .iter()
                    .map(u8::to_ascii_lowercase)
                    .cmp(right.as_bytes().iter().map(u8::to_ascii_lowercase)))
            } else {
                Ok(left.as_bytes().cmp(right.as_bytes()))
            }
        }
        _ => Ok(compare(left, right, context.diagnostics())?.cmp(&0)),
    }
}
