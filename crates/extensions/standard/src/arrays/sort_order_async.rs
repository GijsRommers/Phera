//! Awaitable zend_sort traversal: `usort($a, fn($a, $b) => $a <=> $b)`.
//!
//! PHP comparisons across mixed types are not transitive. Using slice::sort_by
//! would change the comparison sequence and therefore observable results. The
//! explicit partition stack and bounded scans also permit inconsistent callbacks.
/*
 * Copyright (c) 2009-2012 by the contributors listed at
 * http://llvm.org/svn/llvm-project/libcxx/trunk/CREDITS.TXT
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to
 * deal in the Software without restriction, including without limitation the
 * rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
 * sell copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
 * FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
 * IN THE SOFTWARE.
 */
// Safe Rust adaptation of PHP zend_sort, including its small-run traversal.

use std::cmp::Ordering;

use phera_extension_api::NativeError;

const INSERTION_SORT_LIMIT: usize = 16;

const FIVE_SAMPLE_THRESHOLD: usize = 1024;

const SMALL_SORT_LIMIT: usize = 5;

/// Compare original ordinals, so even inconsistent PHP comparisons never violate
/// a Rust sorting API's ordering preconditions.
pub async fn order(
    length: usize,
    mut compare: impl AsyncFnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<Vec<usize>, NativeError> {
    let mut indices: Vec<usize> = (0..length).collect();
    let mut pending = vec![(0, length)];
    while let Some((start, end)) = pending.pop() {
        let length = end.saturating_sub(start);
        if length <= INSERTION_SORT_LIMIT {
            insertion(&mut indices, start, end, &mut compare).await?;
            continue;
        }
        let left = partition(&mut indices, start, end, &mut compare).await?;
        let lower = (start, left - 1);
        let upper = (left, end);
        if lower.1 - lower.0 < upper.1 - upper.0 {
            pending.push(upper);
            pending.push(lower);
        } else {
            pending.push(lower);
            pending.push(upper);
        }
    }
    Ok(indices)
}

/// Bounds-checked reads keep comparator calls safe even on non-transitive values.
async fn greater(
    indices: &[usize],
    left: usize,
    right: usize,
    compare: &mut impl AsyncFnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<bool, NativeError> {
    let (Some(left), Some(right)) = (indices.get(left), indices.get(right)) else {
        return Ok(false);
    };
    Ok(compare(*left, *right).await? == Ordering::Greater)
}

/// PHP uses a three-element decision tree before extending small sorted runs.
async fn small(
    indices: &mut [usize],
    positions: &[usize],
    compare: &mut impl AsyncFnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<(), NativeError> {
    let [first, second, third, ..] = positions else {
        if let [first, second] = positions
            && greater(indices, *first, *second, compare).await?
        {
            swap_positions(indices, *first, *second);
        }
        return Ok(());
    };
    if !greater(indices, *first, *second, compare).await? {
        if greater(indices, *second, *third, compare).await? {
            swap_positions(indices, *second, *third);
            if greater(indices, *first, *second, compare).await? {
                swap_positions(indices, *first, *second);
            }
        }
    } else if !greater(indices, *third, *second, compare).await? {
        swap_positions(indices, *first, *third);
    } else {
        swap_positions(indices, *first, *second);
        if greater(indices, *second, *third, compare).await? {
            swap_positions(indices, *second, *third);
        }
    }
    for position in 3..positions.len() {
        let mut current = position;
        while current > 0 {
            let (Some(previous), Some(next)) = (positions.get(current - 1), positions.get(current))
            else {
                break;
            };
            if !greater(indices, *previous, *next, compare).await? {
                break;
            }
            swap_positions(indices, *previous, *next);
            current -= 1;
        }
    }
    Ok(())
}

/// PHP scans later insertion positions two entries at a time; this is observable
/// for NaN and mixed boolean comparisons that do not form a total order.
async fn insertion(
    indices: &mut [usize],
    start: usize,
    end: usize,
    compare: &mut impl AsyncFnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<(), NativeError> {
    if end.saturating_sub(start) <= SMALL_SORT_LIMIT {
        return small(indices, &(start..end).collect::<Vec<_>>(), compare).await;
    }
    for index in start + 1..end {
        let mut position = index - 1;
        if !greater(indices, position, index, compare).await? {
            continue;
        }
        if index < start + SMALL_SORT_LIMIT + 1 {
            while position > start {
                position -= 1;
                if !greater(indices, position, index, compare).await? {
                    position += 1;
                    break;
                }
            }
        } else {
            loop {
                position = position.saturating_sub(2).max(start);
                if !greater(indices, position, index, compare).await? {
                    position += 1;
                    if !greater(indices, position, index, compare).await? {
                        position += 1;
                    }
                    break;
                }
                if position == start {
                    break;
                }
                if position == start + 1 {
                    position -= 1;
                    if greater(indices, index, position, compare).await? {
                        position += 1;
                    }
                    break;
                }
            }
        }
        for current in (position + 1..=index).rev() {
            swap_positions(indices, current, current - 1);
        }
    }
    Ok(())
}

/// Sampling and inward scans follow zend_sort exactly; each scan shrinks its bounds.
async fn partition(
    indices: &mut [usize],
    start: usize,
    end: usize,
    compare: &mut impl AsyncFnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<usize, NativeError> {
    select_pivot(indices, start, end, compare).await?;
    let pivot = start + 1;
    let mut left = start + 2;
    let mut right = end - 1;
    'partition: loop {
        while greater(indices, pivot, left, compare).await? {
            left += 1;
            if left == right {
                break 'partition;
            }
        }
        right -= 1;
        if right == left {
            break;
        }
        while greater(indices, right, pivot, compare).await? {
            right -= 1;
            if right == left {
                break 'partition;
            }
        }
        swap_positions(indices, left, right);
        left += 1;
        if left == right {
            break;
        }
    }
    swap_positions(indices, pivot, left - 1);
    Ok(left)
}

/// Large partitions use five samples; smaller partitions use three, as Zend does.
async fn select_pivot(
    indices: &mut [usize],
    start: usize,
    end: usize,
    compare: &mut impl AsyncFnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<(), NativeError> {
    let length = end.saturating_sub(start);
    let middle = start + length / 2;
    if length >= FIVE_SAMPLE_THRESHOLD {
        let delta = (length / 2) / 2;
        small(
            indices,
            &[start, start + delta, middle, middle + delta, end - 1],
            compare,
        )
        .await?;
    } else {
        small(indices, &[start, middle, end - 1], compare).await?;
    }
    swap_positions(indices, start + 1, middle);
    Ok(())
}

/// Distinct checked borrows keep swaps safe without requiring comparator consistency.
fn swap_positions(indices: &mut [usize], left: usize, right: usize) {
    if left == right {
        return;
    }
    let (lower, upper) = if left < right {
        (left, right)
    } else {
        (right, left)
    };
    let Some((before, after)) = indices.split_at_mut_checked(upper) else {
        return;
    };
    if let (Some(left), Some(right)) = (before.get_mut(lower), after.first_mut()) {
        std::mem::swap(left, right);
    }
}
