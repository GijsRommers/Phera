//! Dense integer array keys can use verified insertion positions before hashing.

use phera_value::{ArrayKey, PhpArray, Value};

/// Mirrors PhpArray::get: holes, nonzero starts and reordered keys use the map.
#[inline(always)]
pub(crate) fn read_integer(array: &PhpArray, index: i64) -> Option<&Value> {
    usize::try_from(index)
        .ok()
        .and_then(|position| array.entry_at(position))
        .filter(|(key, _)| matches!(key, ArrayKey::Int(stored) if *stored == index))
        .map(|(_, value)| value)
        .or_else(|| array.get_int(index))
}
