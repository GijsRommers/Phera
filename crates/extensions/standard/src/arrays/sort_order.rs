//! PHP traversal for synchronous sorts: `sort([2, 1])` compares the same pairs as Zend.
//!
//! Both synchronous and callback sorts use the awaitable zend_sort port. The
//! synchronous adapter only supplies immediately ready comparisons; polling once
//! therefore always completes and never blocks a runtime thread.
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
use std::task::{Context, Poll, Waker};

use phera_extension_api::NativeError;

pub fn order(
    length: usize,
    mut compare: impl FnMut(usize, usize) -> Result<Ordering, NativeError>,
) -> Result<Vec<usize>, NativeError> {
    let future = super::sort_order_async::order(length, async |left, right| compare(left, right));
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(result) => result,
        Poll::Pending => Err(NativeError::error(
            "Synchronous array comparison unexpectedly yielded",
        )),
    }
}
