//! PHP's `$offset` and `$length` arguments, which count from the end when
//! they are negative. PHP functions read them in one of two ways: clamped
//! to what is there, or rejected when they point outside.

use std::ops::Range;

/// The part of a sequence `total` items long that `$offset` and `$length`
/// select, the way `substr`, `strspn`, `substr_replace`, `array_slice` and
/// `array_splice` read them: clamped, never an error.
///
/// A negative offset counts from the end, a missing length means "up to the
/// end", and a negative length leaves that many items off the end.
///
/// ```php
/// substr("abcdef", -2);    // "ef"
/// substr("abcdef", 1, -2); // "bcd"
/// substr("abc", 5);        // ""
/// ```
pub(crate) fn clamped_range(total: usize, offset: i64, length: Option<i64>) -> Range<usize> {
    let start = if offset < 0 {
        total.saturating_sub(offset.unsigned_abs() as usize)
    } else {
        total.min(offset as usize)
    };
    let end = match length {
        None => total,
        Some(length) if length < 0 => total
            .saturating_sub(length.unsigned_abs() as usize)
            .max(start),
        Some(length) => total.min(start.saturating_add(length as usize)),
    };
    start..end
}

/// The position `$offset` names in a string `total` bytes long, the way
/// `strpos`, `strrpos` and `substr_count` read it, or `None` when it lies
/// outside, which they report as a ValueError.
///
/// ```php
/// strpos("abc", "c", -1); // 2: the search starts one byte before the end
/// strpos("abc", "c", 4);  // ValueError
/// ```
pub(crate) fn contained_offset(total: usize, offset: i64) -> Option<usize> {
    if offset.unsigned_abs() > total as u64 {
        return None;
    }
    Some(if offset < 0 {
        total - offset.unsigned_abs() as usize
    } else {
        offset as usize
    })
}
