//! PHP-compatible comparison traversal with stable ordinal tie breaking.
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
use phera_extension_api::NativeError;
use std::cmp::Ordering;

/// Compare original ordinals, so even inconsistent PHP comparisons never violate
/// a Rust sorting API's ordering preconditions.
pub(super) fn order(
    length: usize,
    mut compare: impl FnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<Vec<usize>, NativeError> {
    let mut indices: Vec<usize> = (0..length).collect();
    let mut pending = vec![(0, length)];
    while let Some((start, end)) = pending.pop() {
        let length = end.saturating_sub(start);
        if length <= 16 {
            insertion(&mut indices, start, end, &mut compare)?;
            continue;
        }
        let middle = start + length / 2;
        if length >= 1024 {
            let delta = (length / 2) / 2;
            small(
                &mut indices,
                &[start, start + delta, middle, middle + delta, end - 1],
                &mut compare,
            )?;
        } else {
            small(&mut indices, &[start, middle, end - 1], &mut compare)?;
        }
        indices.swap(start + 1, middle);
        let pivot = start + 1;
        let mut left = start + 2;
        let mut right = end - 1;
        'partition: loop {
            while greater(&indices, pivot, left, &mut compare)? {
                left += 1;
                if left == right {
                    break 'partition;
                }
            }
            right -= 1;
            if right == left {
                break;
            }
            while greater(&indices, right, pivot, &mut compare)? {
                right -= 1;
                if right == left {
                    break 'partition;
                }
            }
            indices.swap(left, right);
            left += 1;
            if left == right {
                break;
            }
        }
        indices.swap(pivot, left - 1);
        let a = (start, left - 1);
        let b = (left, end);
        if a.1 - a.0 < b.1 - b.0 {
            pending.push(b);
            pending.push(a);
        } else {
            pending.push(a);
            pending.push(b);
        }
    }
    Ok(indices)
}
/// Bounds-checked reads keep comparator calls safe even on non-transitive values.
fn greater(
    indices: &[usize],
    left: usize,
    right: usize,
    compare: &mut impl FnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<bool, NativeError> {
    let (Some(left), Some(right)) = (indices.get(left), indices.get(right)) else {
        return Ok(false);
    };
    Ok(compare(*left, *right)? == Ordering::Greater)
}
/// PHP uses a three-element decision tree before extending small sorted runs.
fn small(
    indices: &mut [usize],
    positions: &[usize],
    compare: &mut impl FnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<(), NativeError> {
    let [a, b, c, ..] = positions else {
        if let [a, b] = positions
            && greater(indices, *a, *b, compare)?
        {
            indices.swap(*a, *b);
        }
        return Ok(());
    };
    if !greater(indices, *a, *b, compare)? {
        if greater(indices, *b, *c, compare)? {
            indices.swap(*b, *c);
            if greater(indices, *a, *b, compare)? {
                indices.swap(*a, *b);
            }
        }
    } else if !greater(indices, *c, *b, compare)? {
        indices.swap(*a, *c);
    } else {
        indices.swap(*a, *b);
        if greater(indices, *b, *c, compare)? {
            indices.swap(*b, *c);
        }
    }
    for position in 3..positions.len() {
        let mut current = position;
        while current > 0 {
            let (Some(previous), Some(next)) = (positions.get(current - 1), positions.get(current))
            else {
                break;
            };
            if !greater(indices, *previous, *next, compare)? {
                break;
            }
            indices.swap(*previous, *next);
            current -= 1;
        }
    }
    Ok(())
}
/// PHP scans later insertion positions two entries at a time; this is observable
/// for NaN and mixed boolean comparisons that do not form a total order.
fn insertion(
    indices: &mut [usize],
    start: usize,
    end: usize,
    compare: &mut impl FnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<(), NativeError> {
    if end - start <= 5 {
        return small(indices, &(start..end).collect::<Vec<_>>(), compare);
    }
    for index in start + 1..end {
        let mut position = index - 1;
        if !greater(indices, position, index, compare)? {
            continue;
        }
        if index < start + 6 {
            while position > start {
                position -= 1;
                if !greater(indices, position, index, compare)? {
                    position += 1;
                    break;
                }
            }
        } else {
            loop {
                position = position.saturating_sub(2).max(start);
                if !greater(indices, position, index, compare)? {
                    position += 1;
                    if !greater(indices, position, index, compare)? {
                        position += 1;
                    }
                    break;
                }
                if position == start {
                    break;
                }
                if position == start + 1 {
                    position -= 1;
                    if greater(indices, index, position, compare)? {
                        position += 1;
                    }
                    break;
                }
            }
        }
        for current in (position + 1..=index).rev() {
            indices.swap(current, current - 1);
        }
    }
    Ok(())
}
