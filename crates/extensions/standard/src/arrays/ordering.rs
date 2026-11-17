//! Shared PHP sorting comparisons.

use std::cmp::Ordering;

use phera_extension_api::value::{compare, to_float, to_php_string};
use phera_extension_api::{Context, NativeError, Value};

/// PHP masks SORT_FLAG_CASE independently of the base comparison mode.
#[derive(Clone, Copy)]
pub(super) enum SortOrder {
    Regular,
    Numeric,
    String { ignore_case: bool },
    Natural { ignore_case: bool },
}

impl SortOrder {
    /// Unknown flags use regular comparison; locale strings use byte ordering.
    pub(super) fn from_flags(flags: i64) -> Self {
        let ignore_case = flags & 8 != 0;
        match flags & !8 {
            1 => Self::Numeric,
            2 | 5 => Self::String { ignore_case },
            6 => Self::Natural { ignore_case },
            _ => Self::Regular,
        }
    }
}

/// Unknown flags fall back to regular comparison; locale strings use byte order.
pub(super) fn compare_values(
    context: &mut dyn Context,
    left: &Value,
    right: &Value,
    order: SortOrder,
) -> Result<Ordering, NativeError> {
    match order {
        SortOrder::Numeric => {
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
        SortOrder::String { ignore_case } | SortOrder::Natural { ignore_case } => {
            let left = to_php_string(left, context.diagnostics())?;
            let right = to_php_string(right, context.diagnostics())?;
            if matches!(order, SortOrder::Natural { .. }) {
                return Ok(crate::natural::natural_compare(
                    left.as_bytes(),
                    right.as_bytes(),
                    ignore_case,
                ));
            }
            if ignore_case {
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
