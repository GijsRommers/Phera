//! Dense integer array keys can use verified insertion positions before hashing.

use phera_value::{PhpArray, Value};

/// Mirrors `PhpArray::get_int` directly without cloning map keys on misses.
#[inline(always)]
pub(crate) fn read_integer(array: &PhpArray, index: i64) -> Option<&Value> {
    array.get_int(index)
}
