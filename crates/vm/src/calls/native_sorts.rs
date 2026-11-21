//! `usort`, `uasort` and `uksort` as native loops: the sort asks for one
//! comparison at a time, and each comparison is a callback on the frame
//! stack.
//!
//! ```php
//! usort($posts, fn ($a, $b) => $b->created <=> $a->created);
//! ```
//!
//! The sort is PHP's own ([`phera_extension_api::zend_sort`]), whose
//! comparisons are observable, so it has to make the same ones in the same
//! order. It is written as `async` code that awaits each comparison. Here
//! it is kept as one boxed future per sort, and its comparison never
//! waits for anything but the answer: it leaves the question in
//! [`Comparisons`] and stays pending until the answer is there. So each
//! poll runs the sort up to its next comparison, the native loop calls the
//! callback for that pair as an ordinary PHP frame, and the callback's
//! return puts the answer in and polls again. When the sort is done, the
//! sorted copy goes into the variable, as the general `usort` does.
//!
//! The rules of the general `usort` (`arrays/callback_sort.rs` in the
//! standard extension) hold here too: a bool result is deprecated and a
//! `false` asks again with the pair swapped; ties keep the original order;
//! after a failure the sort still finishes, with every later comparison
//! "equal", the sorted copy is stored and then the error passed on.

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use phera_extension_api::{NativeError, zend_sort};
use phera_value::{ArrayKey, Diagnostics, PhpArray, Value};

/// Which of the three, as the general `usort` names them.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UserSort {
    /// `usort`: values, numbered from 0 afterwards.
    Values,
    /// `uasort`: values, each keeping its key.
    ValuesWithKeys,
    /// `uksort`: keys.
    Keys,
}

impl UserSort {
    pub(super) fn of(name: &str) -> Option<Self> {
        match name {
            "usort" => Some(Self::Values),
            "uasort" => Some(Self::ValuesWithKeys),
            "uksort" => Some(Self::Keys),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Values => "usort",
            Self::ValuesWithKeys => "uasort",
            Self::Keys => "uksort",
        }
    }
}

/// The comparison the sort waits for, and its answer.
#[derive(Default)]
struct Comparisons {
    asked: Cell<Option<(usize, usize)>>,
    answer: Cell<Option<Ordering>>,
}

type Sorting = Pin<Box<dyn Future<Output = Result<Vec<usize>, NativeError>>>>;

/// A user sort between two comparisons.
pub(super) struct NativeSort {
    sort: UserSort,
    /// The variable `usort` was given, which gets the sorted copy.
    variable: Rc<RefCell<Value>>,
    /// A copy of the entries, so that the callback may even change the
    /// variable while the sort runs.
    entries: Vec<(ArrayKey, Value)>,
    sorting: Sorting,
    comparisons: Rc<Comparisons>,
    /// The pair whose callback runs.
    pair: (usize, usize),
    /// Whether that callback is the second one, with the pair swapped,
    /// after the first answered `false`.
    swapped: bool,
    /// Whether the bool deprecation was raised.
    deprecated: bool,
    /// Whether a comparison failed: every later one is "equal".
    failed: bool,
    /// The first error of a comparison, passed on when the sort is done.
    failure: Option<NativeError>,
}

/// What the sort needs next.
pub(super) enum SortStep {
    /// A callback with these arguments.
    Compare([Value; 2]),
    /// Nothing: it is done, and the variable holds the sorted copy.
    Done(Result<(), NativeError>),
}

impl NativeSort {
    /// A sort of `input`, the array in `variable`.
    pub(super) fn new(sort: UserSort, input: &PhpArray, variable: Rc<RefCell<Value>>) -> Self {
        let entries: Vec<(ArrayKey, Value)> = input
            .iter()
            .map(|(key, value)| (key, value.clone()))
            .collect();
        let comparisons = Rc::new(Comparisons::default());
        let asking = comparisons.clone();
        let sorting = Box::pin(zend_sort::order_async(
            entries.len(),
            async move |left, right| {
                asking.asked.set(Some((left, right)));
                Ok(poll_fn(|_| match asking.answer.take() {
                    Some(answer) => Poll::Ready(answer),
                    None => Poll::Pending,
                })
                .await)
            },
        ));
        Self {
            sort,
            variable,
            entries,
            sorting,
            comparisons,
            pair: (0, 0),
            swapped: false,
            deprecated: false,
            failed: false,
            failure: None,
        }
    }

    /// Runs the sort up to its next comparison that needs a callback.
    pub(super) fn next_step(&mut self) -> SortStep {
        loop {
            let polled = self
                .sorting
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
            match polled {
                Poll::Ready(order) => return SortStep::Done(self.store(order)),
                Poll::Pending => {}
            }
            let Some(pair) = self.comparisons.asked.take() else {
                return SortStep::Done(Err(internal_error!("user sort")));
            };
            if self.failed {
                // After a failure every comparison is "equal".
                self.answer(pair, Ordering::Equal);
                continue;
            }
            self.pair = pair;
            self.swapped = false;
            return SortStep::Compare(self.arguments());
        }
    }

    /// The callback's arguments for the pair.
    fn arguments(&self) -> [Value; 2] {
        let (left, right) = self.pair;
        let (left, right) = match self.swapped {
            false => (left, right),
            true => (right, left),
        };
        [self.argument(left), self.argument(right)]
    }

    fn argument(&self, index: usize) -> Value {
        match (self.sort, self.entries.get(index)) {
            (UserSort::Keys, Some((key, _))) => Value::from(key.clone()),
            (_, Some((_, value))) => value.clone(),
            (_, None) => Value::Null,
        }
    }

    /// What the callback for the pair returned. `Some` with the next
    /// callback's arguments when the sort needs one more call for this
    /// pair (a `false` asks again, swapped); `None` once the pair is
    /// answered, for [`NativeSort::next_step`].
    pub(super) fn answered(
        &mut self,
        result: &Value,
        diagnostics: &mut dyn Diagnostics,
    ) -> Option<[Value; 2]> {
        if !self.swapped
            && let Value::Bool(greater) = result
        {
            if !self.deprecated {
                diagnostics.deprecated(format!(
                    "{}(): Returning bool from comparison function is deprecated, \
                     return an integer less than, equal to, or greater than zero",
                    self.sort.name()
                ));
                self.deprecated = true;
            }
            if !greater {
                self.swapped = true;
                return Some(self.arguments());
            }
        }
        let order = match phera_value::to_int(result, diagnostics) {
            Ok(number) if self.swapped => number.cmp(&0).reverse(),
            Ok(number) => number.cmp(&0),
            Err(error) => {
                self.failed = true;
                self.failure = Some(error.into());
                Ordering::Equal
            }
        };
        self.answer(self.pair, order);
        None
    }

    /// The callback for the pair failed (it threw, or could not be
    /// entered): the sort finishes without it and the sorted copy is
    /// stored. The error itself is the caller's to pass on.
    pub(super) fn abandon(mut self) {
        self.failed = true;
        self.answer(self.pair, Ordering::Equal);
        // After a failure every comparison is answered at once, so this
        // finishes without a callback.
        let _ = self.next_step();
    }

    /// PHP 8's sort is stable: a tie keeps the original order.
    fn answer(&self, (left, right): (usize, usize), order: Ordering) {
        self.comparisons
            .answer
            .set(Some(order.then_with(|| left.cmp(&right))));
    }

    /// Stores the sorted copy in the variable. Like every PHP sort, its
    /// internal pointer is at the first entry.
    fn store(&mut self, order: Result<Vec<usize>, NativeError>) -> Result<(), NativeError> {
        let order = order?;
        let mut sorted = PhpArray::with_capacity(self.entries.len());
        let mut entries: Vec<Option<(ArrayKey, Value)>> = std::mem::take(&mut self.entries)
            .into_iter()
            .map(Some)
            .collect();
        for index in order {
            let Some((key, value)) = entries.get_mut(index).and_then(Option::take) else {
                return Err(internal_error!("user sort order"));
            };
            if self.sort == UserSort::Values {
                sorted.push(value)?;
            } else {
                sorted.insert(key, value);
            }
        }
        sorted.reset();
        let Ok(mut variable) = self.variable.try_borrow_mut() else {
            return Err(internal_error!("user sort variable"));
        };
        *variable = Value::from(sorted);
        drop(variable);
        match self.failure.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}
