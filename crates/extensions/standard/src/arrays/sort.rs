//! Sorting in place: `sort`, `rsort`, `asort`, `arsort`, `ksort`, `krsort`,
//! `natsort` and `natcasesort`.
//!
//! All of them use PHP's own algorithm ([`super::zend_sort`]) so that they
//! make the same comparisons, and like PHP 8 they are stable: entries that
//! compare equal keep their order, also when sorting in reverse.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray, Value,
    args,
};

use super::ordering::SortFlags;
use super::{IntegerKeys, zend_sort};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[
    SORT,
    RSORT,
    ASORT,
    ARSORT,
    KSORT,
    KRSORT,
    NATSORT,
    NATCASESORT,
];

/// What one of the sort functions sorts, and how.
#[derive(Clone, Copy)]
struct Sort {
    by: SortBy,
    reverse: bool,
    keys: IntegerKeys,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortBy {
    Values,
    Keys,
}

/// `sort(array &$array, int $flags = SORT_REGULAR): true`, the values in
/// ascending order, numbered from 0.
///
/// ```php
/// $a = [3, "10", 2];
/// sort($a);              // $a is [2, 3, "10"]
/// sort($a, SORT_STRING); // $a is ["10", 2, 3]
/// ```
const SORT: FunctionEntry = FunctionEntry::asynchronous(
    "sort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    sort,
);

fn sort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let sort = Sort {
        by: SortBy::Values,
        reverse: false,
        keys: IntegerKeys::Renumber,
    };
    let flags = flags_argument(&arguments);
    Box::pin(sort_array(context, arguments, sort, flags))
}

/// `rsort()`, `sort()` in descending order.
const RSORT: FunctionEntry = FunctionEntry::asynchronous(
    "rsort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    rsort,
);

fn rsort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let sort = Sort {
        by: SortBy::Values,
        reverse: true,
        keys: IntegerKeys::Renumber,
    };
    let flags = flags_argument(&arguments);
    Box::pin(sort_array(context, arguments, sort, flags))
}

/// `asort()`, `sort()` keeping each value's key.
///
/// ```php
/// $a = ["x" => 3, "y" => 1];
/// asort($a); // $a is ["y" => 1, "x" => 3]
/// ```
const ASORT: FunctionEntry = FunctionEntry::asynchronous(
    "asort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    asort,
);

fn asort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let sort = Sort {
        by: SortBy::Values,
        reverse: false,
        keys: IntegerKeys::Preserve,
    };
    let flags = flags_argument(&arguments);
    Box::pin(sort_array(context, arguments, sort, flags))
}

/// `arsort()`, `asort()` in descending order.
const ARSORT: FunctionEntry = FunctionEntry::asynchronous(
    "arsort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    arsort,
);

fn arsort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let sort = Sort {
        by: SortBy::Values,
        reverse: true,
        keys: IntegerKeys::Preserve,
    };
    let flags = flags_argument(&arguments);
    Box::pin(sort_array(context, arguments, sort, flags))
}

/// `ksort(array &$array, int $flags = SORT_REGULAR): true`, the entries in
/// ascending order of their keys.
///
/// ```php
/// $a = ["b" => 1, "a" => 2, 10 => 3];
/// ksort($a); // $a is ["a" => 2, "b" => 1, 10 => 3]
/// ```
const KSORT: FunctionEntry = FunctionEntry::asynchronous(
    "ksort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    ksort,
);

fn ksort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let sort = Sort {
        by: SortBy::Keys,
        reverse: false,
        keys: IntegerKeys::Preserve,
    };
    let flags = flags_argument(&arguments);
    Box::pin(sort_array(context, arguments, sort, flags))
}

/// `krsort()`, `ksort()` in descending order.
const KRSORT: FunctionEntry = FunctionEntry::asynchronous(
    "krsort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    krsort,
);

fn krsort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let sort = Sort {
        by: SortBy::Keys,
        reverse: true,
        keys: IntegerKeys::Preserve,
    };
    let flags = flags_argument(&arguments);
    Box::pin(sort_array(context, arguments, sort, flags))
}

/// `natsort(array &$array): true`, `asort()` in `strnatcmp` order.
///
/// ```php
/// $a = ["img12", "img10", "img2"];
/// natsort($a); // $a is [2 => "img2", 1 => "img10", 0 => "img12"]
/// ```
const NATSORT: FunctionEntry = FunctionEntry::asynchronous(
    "natsort",
    &[Param::new("array", ParamType::Array).by_ref()],
    natsort,
);

fn natsort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let sort = Sort {
        by: SortBy::Values,
        reverse: false,
        keys: IntegerKeys::Preserve,
    };
    let flags = SortFlags::Natural { ignore_case: false };
    Box::pin(sort_array(context, arguments, sort, flags))
}

/// `natcasesort()`, `natsort()` ignoring ASCII case.
const NATCASESORT: FunctionEntry = FunctionEntry::asynchronous(
    "natcasesort",
    &[Param::new("array", ParamType::Array).by_ref()],
    natcasesort,
);

fn natcasesort(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    let sort = Sort {
        by: SortBy::Values,
        reverse: false,
        keys: IntegerKeys::Preserve,
    };
    let flags = SortFlags::Natural { ignore_case: true };
    Box::pin(sort_array(context, arguments, sort, flags))
}

fn flags_argument(arguments: &[Value]) -> SortFlags {
    SortFlags::from_flags(args::int(arguments, 1).unwrap_or_default())
}

async fn sort_array(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    sort: Sort,
    flags: SortFlags,
) -> NativeResult {
    let variable = args::reference(&arguments, 0).ok_or_else(super::not_a_variable)?;
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
    let mut failure = None;
    let order = zend_sort::order_async(entries.len(), async |left, right| {
        let (left_entry, right_entry) = (&entries[left], &entries[right]);
        let order = if failure.is_some() {
            std::cmp::Ordering::Equal
        } else {
            let comparison = match sort.by {
                SortBy::Values => {
                    flags
                        .compare_with_objects(context, &left_entry.1, &right_entry.1)
                        .await
                }
                SortBy::Keys => flags.compare(
                    context,
                    &Value::from(left_entry.0.clone()),
                    &Value::from(right_entry.0.clone()),
                ),
            };
            match comparison {
                Ok(order) => order,
                Err(error) => {
                    failure = Some(error);
                    std::cmp::Ordering::Equal
                }
            }
        };
        let order = if sort.reverse { order.reverse() } else { order };
        Ok(order.then_with(|| left.cmp(&right)))
    })
    .await?;
    let mut result = PhpArray::with_capacity(entries.len());
    for index in order {
        let (key, value) = &entries[index];
        match sort.keys {
            IntegerKeys::Preserve => {
                result.insert(key.clone(), value.clone());
            }
            IntegerKeys::Renumber => {
                result.push(value.clone())?;
            }
        }
    }
    result.reset();
    *variable
        .try_borrow_mut()
        .map_err(|_| super::variable_in_use())? = Value::from(result);
    failure.map_or(Ok(Value::Bool(true)), Err)
}
