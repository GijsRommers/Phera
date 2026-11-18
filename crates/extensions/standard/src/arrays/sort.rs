//! Sorting in place: `sort`, `rsort`, `asort`, `arsort`, `ksort`, `krsort`,
//! `natsort` and `natcasesort`.
//!
//! All of them use PHP's own algorithm ([`super::zend_sort`]) so that they
//! make the same comparisons, and like PHP 8 they are stable: entries that
//! compare equal keep their order, also when sorting in reverse.

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Value, args,
};

use super::ordering::SortFlags;
use super::{IntegerKeys, change_array_argument, zend_sort};

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
const SORT: FunctionEntry = FunctionEntry::sync(
    "sort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    sort,
);

fn sort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let sort = Sort {
        by: SortBy::Values,
        reverse: false,
        keys: IntegerKeys::Renumber,
    };
    sort_array(context, arguments, sort, flags_argument(arguments))
}

/// `rsort()`, `sort()` in descending order.
const RSORT: FunctionEntry = FunctionEntry::sync(
    "rsort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    rsort,
);

fn rsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let sort = Sort {
        by: SortBy::Values,
        reverse: true,
        keys: IntegerKeys::Renumber,
    };
    sort_array(context, arguments, sort, flags_argument(arguments))
}

/// `asort()`, `sort()` keeping each value's key.
///
/// ```php
/// $a = ["x" => 3, "y" => 1];
/// asort($a); // $a is ["y" => 1, "x" => 3]
/// ```
const ASORT: FunctionEntry = FunctionEntry::sync(
    "asort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    asort,
);

fn asort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let sort = Sort {
        by: SortBy::Values,
        reverse: false,
        keys: IntegerKeys::Preserve,
    };
    sort_array(context, arguments, sort, flags_argument(arguments))
}

/// `arsort()`, `asort()` in descending order.
const ARSORT: FunctionEntry = FunctionEntry::sync(
    "arsort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    arsort,
);

fn arsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let sort = Sort {
        by: SortBy::Values,
        reverse: true,
        keys: IntegerKeys::Preserve,
    };
    sort_array(context, arguments, sort, flags_argument(arguments))
}

/// `ksort(array &$array, int $flags = SORT_REGULAR): true`, the entries in
/// ascending order of their keys.
///
/// ```php
/// $a = ["b" => 1, "a" => 2, 10 => 3];
/// ksort($a); // $a is ["a" => 2, "b" => 1, 10 => 3]
/// ```
const KSORT: FunctionEntry = FunctionEntry::sync(
    "ksort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    ksort,
);

fn ksort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let sort = Sort {
        by: SortBy::Keys,
        reverse: false,
        keys: IntegerKeys::Preserve,
    };
    sort_array(context, arguments, sort, flags_argument(arguments))
}

/// `krsort()`, `ksort()` in descending order.
const KRSORT: FunctionEntry = FunctionEntry::sync(
    "krsort",
    &[
        Param::new("array", ParamType::Array).by_ref(),
        Param::new("flags", ParamType::Int).optional(),
    ],
    krsort,
);

fn krsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let sort = Sort {
        by: SortBy::Keys,
        reverse: true,
        keys: IntegerKeys::Preserve,
    };
    sort_array(context, arguments, sort, flags_argument(arguments))
}

/// `natsort(array &$array): true`, `asort()` in `strnatcmp` order.
///
/// ```php
/// $a = ["img12", "img10", "img2"];
/// natsort($a); // $a is [2 => "img2", 1 => "img10", 0 => "img12"]
/// ```
const NATSORT: FunctionEntry = FunctionEntry::sync(
    "natsort",
    &[Param::new("array", ParamType::Array).by_ref()],
    natsort,
);

fn natsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let sort = Sort {
        by: SortBy::Values,
        reverse: false,
        keys: IntegerKeys::Preserve,
    };
    let flags = SortFlags::Natural { ignore_case: false };
    sort_array(context, arguments, sort, flags)
}

/// `natcasesort()`, `natsort()` ignoring ASCII case.
const NATCASESORT: FunctionEntry = FunctionEntry::sync(
    "natcasesort",
    &[Param::new("array", ParamType::Array).by_ref()],
    natcasesort,
);

fn natcasesort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let sort = Sort {
        by: SortBy::Values,
        reverse: false,
        keys: IntegerKeys::Preserve,
    };
    let flags = SortFlags::Natural { ignore_case: true };
    sort_array(context, arguments, sort, flags)
}

fn flags_argument(arguments: &[Value]) -> SortFlags {
    SortFlags::from_flags(args::int(arguments, 1).unwrap_or_default())
}

fn sort_array(
    context: &mut dyn Context,
    arguments: &[Value],
    sort: Sort,
    flags: SortFlags,
) -> NativeResult {
    change_array_argument(arguments, |array| {
        let entries: Vec<(ArrayKey, Value)> = array
            .iter()
            .map(|(key, value)| (key, value.clone()))
            .collect();
        let order = zend_sort::order(entries.len(), |left, right| {
            let (left_entry, right_entry) = (&entries[left], &entries[right]);
            let order = match sort.by {
                SortBy::Values => flags.compare(context, &left_entry.1, &right_entry.1)?,
                SortBy::Keys => flags.compare(
                    context,
                    &Value::from(left_entry.0.clone()),
                    &Value::from(right_entry.0.clone()),
                )?,
            };
            let order = if sort.reverse { order.reverse() } else { order };
            // PHP 8's stable sort: equal entries stay in their old order.
            Ok(order.then_with(|| left.cmp(&right)))
        })?;
        let mut result = PhpArray::new();
        for index in order {
            let (key, value) = &entries[index];
            if sort.keys == IntegerKeys::Preserve {
                result.insert(key.clone(), value.clone());
            } else {
                result.push(value.clone())?;
            }
        }
        result.reset();
        *array = result;
        Ok(Value::Bool(true))
    })
}
