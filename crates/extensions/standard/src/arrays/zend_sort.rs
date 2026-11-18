//! PHP's sort algorithm, `zend_sort` from php-src `Zend/zend_sort.c`, ported
//! comparison for comparison.
//!
//! Why not `slice::sort_by`? Because PHP's sort is observable. A callback
//! passed to `usort` sees every comparison, and PHP's own comparisons are not
//! a total order (`NAN`, or `"abc" < "abd" < 10 < "abc"` across types), so
//! which pairs get compared decides the result. Making exactly the
//! comparisons `zend_sort` makes, in the same order, is the only way to get
//! PHP's answers.
//!
//! Both functions here sort the positions `0..length` and ask `compare` about
//! two positions at a time; the caller maps a position to its own entry. A
//! comparison that is not even consistent cannot break the algorithm: every
//! loop is bounded by slot numbers, never by what the comparisons answered.
//!
//! There is one implementation, and it is `async`, because `usort` compares
//! by calling PHP code, which may wait. [`order`] serves `sort()`, whose
//! comparisons are plain Rust: see there why driving the same `async` code
//! by hand is sound.
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

use std::cmp::Ordering;
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Poll, Waker};

use phera_extension_api::NativeError;

/// Runs of at most this many entries are insertion sorted (`nmemb <= 16`).
const INSERTION_SORT_MAX: usize = 16;

/// From this length on, the pivot is the median of five samples rather than
/// three (`nmemb >> 10` is nonzero).
const FIVE_SAMPLES_MIN: usize = 1024;

/// Up to five entries, `zend_insert_sort` uses the fixed comparison trees
/// `zend_sort_2` to `zend_sort_5`. In a longer run it inserts the entries up
/// to this slot after `start` one slot at a time, and the later ones in steps
/// of two.
const FIXED_TREE_MAX: usize = 5;

/// The sorted order of `0..length` for a comparison that never waits, as
/// `sort()` and `array_unique()` need it.
///
/// `order_async` waits only where it awaits `compare`, and the comparison
/// built here returns without waiting, so the first poll always finishes.
/// That is why a waker that does nothing is enough.
pub fn order(
    length: usize,
    mut compare: impl FnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<Vec<usize>, NativeError> {
    let mut sorting = pin!(order_async(length, async |left, right| compare(
        left, right
    )));
    match sorting
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => {
            unreachable!("a comparison that never waits cannot leave the sort waiting")
        }
    }
}

/// The sorted order of `0..length`, for comparisons that may wait, such as
/// `usort`'s PHP callback.
pub async fn order_async(
    length: usize,
    compare: impl AsyncFnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<Vec<usize>, NativeError> {
    let mut sorter = Sorter {
        entries: (0..length).collect(),
        compare,
    };
    // zend_sort recurses into the smaller part and then loops on the larger
    // one. A stack that holds the larger part below the smaller one makes the
    // same comparisons in the same order, without Rust recursion.
    let mut runs = vec![(0, length)];
    while let Some((start, end)) = runs.pop() {
        if end - start <= INSERTION_SORT_MAX {
            sorter.insertion_sort(start, end).await?;
            continue;
        }
        let split = sorter.partition(start, end).await?;
        // The pivot sits at `split - 1`, between the two parts.
        let lower = (start, split - 1);
        let upper = (split, end);
        if lower.1 - lower.0 < upper.1 - upper.0 {
            runs.push(upper);
            runs.push(lower);
        } else {
            runs.push(lower);
            runs.push(upper);
        }
    }
    Ok(sorter.entries)
}

struct Sorter<C> {
    /// The entry in each slot. The slots start as `0, 1, 2, …` and end in
    /// sorted order.
    entries: Vec<usize>,
    compare: C,
}

impl<C: AsyncFnMut(usize, usize) -> Result<Ordering, NativeError>> Sorter<C> {
    /// `cmp(a, b) > 0` for the entries in two slots, the only question
    /// zend_sort ever asks.
    async fn greater(&mut self, left: usize, right: usize) -> Result<bool, NativeError> {
        let order = (self.compare)(self.entries[left], self.entries[right]).await?;
        Ok(order == Ordering::Greater)
    }

    fn swap(&mut self, left: usize, right: usize) {
        self.entries.swap(left, right);
    }

    /// `zend_sort_2` to `zend_sort_5`: sorts the entries in up to five slots
    /// with a fixed comparison tree.
    async fn sort_few(&mut self, slots: &[usize]) -> Result<(), NativeError> {
        match *slots {
            [] | [_] => {}
            [first, second] => {
                if self.greater(first, second).await? {
                    self.swap(first, second);
                }
            }
            [first, second, third, ..] => {
                self.sort_three(first, second, third).await?;
                // zend_sort_4 and zend_sort_5 sink the fourth and fifth entry
                // into place, comparing from the top.
                for next in 3..slots.len() {
                    let mut current = next;
                    while current > 0 && self.greater(slots[current - 1], slots[current]).await? {
                        self.swap(slots[current - 1], slots[current]);
                        current -= 1;
                    }
                }
            }
        }
        Ok(())
    }

    /// `zend_sort_3`, which needs two or three comparisons.
    async fn sort_three(&mut self, a: usize, b: usize, c: usize) -> Result<(), NativeError> {
        if !self.greater(a, b).await? {
            if self.greater(b, c).await? {
                self.swap(b, c);
                if self.greater(a, b).await? {
                    self.swap(a, b);
                }
            }
        } else if !self.greater(c, b).await? {
            self.swap(a, c);
        } else {
            self.swap(a, b);
            if self.greater(b, c).await? {
                self.swap(b, c);
            }
        }
        Ok(())
    }

    /// `zend_insert_sort`, for a run of at most 16 entries.
    async fn insertion_sort(&mut self, start: usize, end: usize) -> Result<(), NativeError> {
        let length = end - start;
        if length <= FIXED_TREE_MAX {
            let slots = [start, start + 1, start + 2, start + 3, start + 4];
            return self.sort_few(&slots[..length]).await;
        }
        for slot in start + 1..end {
            if !self.greater(slot - 1, slot).await? {
                continue;
            }
            let target = if slot <= start + FIXED_TREE_MAX {
                self.insertion_point_by_ones(start, slot).await?
            } else {
                self.insertion_point_by_twos(start, slot).await?
            };
            // Move the entry down to `target`, shifting the ones in between up.
            for current in (target + 1..=slot).rev() {
                self.swap(current, current - 1);
            }
        }
        Ok(())
    }

    /// Where the entry in `slot` belongs among the sorted slots below it,
    /// checking one slot at a time. The slot just below is known to be
    /// greater.
    async fn insertion_point_by_ones(
        &mut self,
        start: usize,
        slot: usize,
    ) -> Result<usize, NativeError> {
        let mut target = slot - 1;
        while target > start && self.greater(target - 1, slot).await? {
            target -= 1;
        }
        Ok(target)
    }

    /// The same, checking every second slot and then the one it skipped.
    /// Only called for `slot > start + 5`, so the steps stop at `start` or
    /// `start + 1` and never go below `start`.
    async fn insertion_point_by_twos(
        &mut self,
        start: usize,
        slot: usize,
    ) -> Result<usize, NativeError> {
        let mut target = slot - 1;
        loop {
            target -= 2;
            if !self.greater(target, slot).await? {
                target += 1;
                if !self.greater(target, slot).await? {
                    target += 1;
                }
                return Ok(target);
            }
            if target == start {
                return Ok(start);
            }
            if target == start + 1 {
                // zend_insert_sort asks about the last slot the other way
                // round: `cmp(i, j) > 0`.
                return Ok(if self.greater(slot, start).await? {
                    start + 1
                } else {
                    start
                });
            }
        }
    }

    /// One partition step of zend_sort: the pivot moves to `start + 1`, the
    /// other entries are split around it, and the pivot ends up just below
    /// the returned slot.
    async fn partition(&mut self, start: usize, end: usize) -> Result<usize, NativeError> {
        self.choose_pivot(start, end).await?;
        let pivot = start + 1;
        let mut left = start + 2;
        let mut right = end - 1;
        'scan: loop {
            while self.greater(pivot, left).await? {
                left += 1;
                if left == right {
                    break 'scan;
                }
            }
            right -= 1;
            if right == left {
                break;
            }
            while self.greater(right, pivot).await? {
                right -= 1;
                if right == left {
                    break 'scan;
                }
            }
            self.swap(left, right);
            left += 1;
            if left == right {
                break;
            }
        }
        self.swap(pivot, left - 1);
        Ok(left)
    }

    /// Sorts three samples (five for long runs), whose middle one becomes the
    /// pivot, and moves it to `start + 1`.
    async fn choose_pivot(&mut self, start: usize, end: usize) -> Result<(), NativeError> {
        let length = end - start;
        let middle = start + length / 2;
        if length >= FIVE_SAMPLES_MIN {
            let delta = length / 4;
            self.sort_few(&[start, start + delta, middle, middle + delta, end - 1])
                .await?;
        } else {
            self.sort_few(&[start, middle, end - 1]).await?;
        }
        self.swap(start + 1, middle);
        Ok(())
    }
}
