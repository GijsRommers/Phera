//! How `sort`, `array_unique` and the set functions compare two values:
//! the comparison that a `SORT_*` flag selects.

use std::cmp::Ordering;

use phera_extension_api::value::{compare, to_float, to_php_string};
use phera_extension_api::{Context, NativeError, Value};

use crate::constants::{
    SORT_FLAG_CASE, SORT_LOCALE_STRING, SORT_NATURAL, SORT_NUMERIC, SORT_STRING,
};
use crate::natural::natural_compare;

pub(super) fn can_invoke_string_comparison(left: &Value, right: &Value) -> bool {
    matches!(
        (&*left.deref(), &*right.deref()),
        (Value::Object(_), _) | (_, Value::Object(_)) | (Value::Array(_), Value::Array(_))
    )
}

/// A `$flags` argument such as `SORT_STRING | SORT_FLAG_CASE`.
#[derive(Clone, Copy)]
pub(super) enum SortFlags {
    /// `SORT_REGULAR`: PHP's `<=>`, with its type juggling.
    Regular,
    /// `SORT_NUMERIC`: both as floats.
    Numeric,
    /// `SORT_STRING`: both as strings, byte by byte.
    String { ignore_case: bool },
    /// `SORT_NATURAL`: both as strings, in `strnatcmp` order.
    Natural { ignore_case: bool },
}

impl SortFlags {
    /// `SORT_STRING`, the comparison of `array_diff`, `array_intersect` and
    /// `array_unique` by default.
    pub(super) const STRING: Self = Self::String { ignore_case: false };

    /// `SORT_FLAG_CASE` only matters for strings. Unknown flags mean
    /// `SORT_REGULAR`, and `SORT_LOCALE_STRING` compares bytes, as PHP does
    /// in the C locale.
    pub(super) fn from_flags(flags: i64) -> Self {
        let ignore_case = flags & SORT_FLAG_CASE != 0;
        match flags & !SORT_FLAG_CASE {
            SORT_NUMERIC => Self::Numeric,
            SORT_STRING | SORT_LOCALE_STRING => Self::String { ignore_case },
            SORT_NATURAL => Self::Natural { ignore_case },
            _ => Self::Regular,
        }
    }

    pub(super) async fn compare_with_objects(
        self,
        context: &mut dyn Context,
        left: &Value,
        right: &Value,
    ) -> Result<Ordering, NativeError> {
        match self {
            Self::Regular if can_invoke_string_comparison(left, right) => {
                Ok(context.compare(left, right).await?.cmp(&0))
            }
            Self::String { .. } | Self::Natural { .. } if left.is_object() || right.is_object() => {
                let left = Value::String(context.stringify(left).await?);
                let right = Value::String(context.stringify(right).await?);
                self.compare(context, &left, &right)
            }
            _ => self.compare(context, left, right),
        }
    }

    pub(super) fn compare(
        self,
        context: &mut dyn Context,
        left: &Value,
        right: &Value,
    ) -> Result<Ordering, NativeError> {
        Ok(match self {
            Self::Regular => compare(left, right, context.diagnostics())?.cmp(&0),
            Self::Numeric => {
                let left = to_float(left, context.diagnostics())?;
                let right = to_float(right, context.diagnostics())?;
                // ZEND_THREEWAY_COMPARE: a NAN on either side is "greater".
                if left == right {
                    Ordering::Equal
                } else if left < right {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            Self::String { ignore_case } => {
                let left = to_php_string(left, context.diagnostics())?;
                let right = to_php_string(right, context.diagnostics())?;
                if ignore_case {
                    let left = left.as_bytes().iter().map(u8::to_ascii_lowercase);
                    left.cmp(right.as_bytes().iter().map(u8::to_ascii_lowercase))
                } else {
                    left.as_bytes().cmp(right.as_bytes())
                }
            }
            Self::Natural { ignore_case } => {
                let left = to_php_string(left, context.diagnostics())?;
                let right = to_php_string(right, context.diagnostics())?;
                natural_compare(left.as_bytes(), right.as_bytes(), ignore_case)
            }
        })
    }
}
