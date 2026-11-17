//! Sorting with a comparison callback: `usort`, `uasort` and `uksort`.
//!
//! ```php
//! $a = [3, 1, 2];
//! usort($a, fn($x, $y) => $x <=> $y); // $a is [1, 2, 3]
//! ```
//!
//! They use PHP's own algorithm ([`super::zend_sort`]), so the callback
//! sees the same pairs in the same order as in PHP.

use std::cmp::Ordering;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray, Value,
    args,
};

use super::callback::{BoolResult, UserComparison, check_callback};
use super::zend_sort;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[USORT, UASORT, UKSORT];

/// What a user sort compares and keeps.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UserSort {
    /// `usort`: values, numbered from 0 afterwards.
    Values,
    /// `uasort`: values, each keeping its key.
    ValuesWithKeys,
    /// `uksort`: keys.
    Keys,
}

/// `usort(array &$array, callable $callback): true`
const USORT: FunctionEntry = FunctionEntry::asynchronous(
    "usort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("callback", ParamType::Callable),
    ],
    usort,
);

fn usort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(sort_with_callback(
        context,
        arguments,
        "usort",
        UserSort::Values,
    ))
}

/// `uasort(array &$array, callable $callback): true`, keeping the keys.
///
/// ```php
/// $a = ["x" => 3, "y" => 1];
/// uasort($a, fn($p, $q) => $p <=> $q); // $a is ["y" => 1, "x" => 3]
/// ```
const UASORT: FunctionEntry = FunctionEntry::asynchronous(
    "uasort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("callback", ParamType::Callable),
    ],
    uasort,
);

fn uasort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(sort_with_callback(
        context,
        arguments,
        "uasort",
        UserSort::ValuesWithKeys,
    ))
}

/// `uksort(array &$array, callable $callback): true`, comparing keys.
///
/// ```php
/// $a = ["b" => 1, "a" => 2];
/// uksort($a, "strcmp"); // $a is ["a" => 2, "b" => 1]
/// ```
const UKSORT: FunctionEntry = FunctionEntry::asynchronous(
    "uksort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("callback", ParamType::Callable),
    ],
    uksort,
);

fn uksort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(sort_with_callback(
        context,
        arguments,
        "uksort",
        UserSort::Keys,
    ))
}

async fn sort_with_callback(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    sort: UserSort,
) -> NativeResult {
    let callback = &arguments[1];
    check_callback(function, callback)?;
    let variable = args::reference(&arguments, 0).ok_or_else(super::not_a_variable)?;
    // A copy of the entries, so that no borrow of the variable is held while
    // the callback runs; it may even change the array.
    let entries: Vec<(ArrayKey, Value)> = match &*variable
        .try_borrow()
        .map_err(|_| super::variable_in_use())?
    {
        Value::Array(array) => array
            .iter()
            .map(|(key, value)| (key, value.clone()))
            .collect(),
        _ => return Err(super::not_a_variable()),
    };
    if entries.is_empty() {
        return Ok(Value::Bool(true));
    }
    let mut comparison = UserComparison {
        function,
        deprecated: false,
    };
    // When the callback throws, zend_sort still finishes, with every later
    // comparison "equal"; the sorted copy is stored, then the error passed on.
    let mut failure = None;
    let order = zend_sort::order_async(entries.len(), async |left, right| {
        if failure.is_none() {
            let (left_entry, right_entry) = (&entries[left], &entries[right]);
            let (left_value, right_value) = match sort {
                UserSort::Keys => (
                    Value::from(left_entry.0.clone()),
                    Value::from(right_entry.0.clone()),
                ),
                UserSort::Values | UserSort::ValuesWithKeys => {
                    (left_entry.1.clone(), right_entry.1.clone())
                }
            };
            let compared = comparison
                .compare(
                    context,
                    callback,
                    left_value,
                    right_value,
                    BoolResult::AskAgain,
                )
                .await;
            match compared {
                Ok(order) => return Ok(order.then_with(|| left.cmp(&right))),
                Err(error) => failure = Some(error),
            }
        }
        // PHP 8's stable sort breaks ties by the original order.
        Ok(Ordering::Equal.then_with(|| left.cmp(&right)))
    })
    .await?;
    let mut sorted = PhpArray::with_capacity(entries.len());
    for index in order {
        let (key, value) = &entries[index];
        if sort == UserSort::Values {
            sorted.push(value.clone())?;
        } else {
            sorted.insert(key.clone(), value.clone());
        }
    }
    // Like every PHP sort, the internal pointer starts at the first entry again.
    sorted.reset();
    *variable.borrow_mut() = Value::from(sorted);
    match failure {
        Some(error) => Err(error),
        None => Ok(Value::Bool(true)),
    }
}
