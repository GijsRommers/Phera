//! An `int` can contain 0, which a `-1|positive-int` return excludes.

use crate::{constant_types::int_bounds, types::ValueType};

pub(super) fn range_excludes_values(expected: &ValueType, given: &ValueType) -> bool {
    if !matches!(given, ValueType::Int | ValueType::IntRange(..)) {
        return false;
    }
    let Some((mut first, last)) = int_bounds(given) else {
        return false;
    };
    if let ValueType::IntRange(min, max) = expected {
        return first < *min || last > *max;
    }
    let ValueType::Union(arms) = expected else {
        return false;
    };
    let Some(mut allowed) = arms.iter().map(int_bounds).collect::<Option<Vec<_>>>() else {
        return false;
    };
    allowed.sort_unstable();
    for (min, max) in allowed {
        if first < min {
            return true;
        }
        if first <= max {
            if last <= max {
                return false;
            }
            first = max + 1;
        }
    }
    true
}
