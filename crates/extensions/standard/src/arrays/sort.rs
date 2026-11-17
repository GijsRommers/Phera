//! Stable array sorting with PHP-compatible comparison traversal.
//!
//! ```php
//! $a = [2, 1]; $a[] = &$a;
//! sort($a); // [1, 2, &$a]
//! ```

use std::cmp::Ordering;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, PhpArray,
    Registry, Value, args,
};

use phera_extension_api::value::compare_after_error;

use super::Keys;
use super::ordering::SortOrder;

#[derive(Clone, Copy)]
enum Direction {
    Ascending,
    Descending,
}

#[derive(Clone, Copy)]
enum SortBy {
    Values,
    Keys,
}

const ARRAY_SORT_PARAMS: &[Param] = &[
    Param::new("array", ParamType::Array).by_ref(),
    Param::new("flags", ParamType::Int).optional(),
];

const ARRAY_NATURAL_PARAMS: &[Param] = &[Param::new("array", ParamType::Array).by_ref()];

pub(crate) fn register(registry: &mut Registry) {
    for (name, body) in [
        ("sort", sort as phera_extension_api::SyncFn),
        ("rsort", rsort),
        ("asort", asort),
        ("arsort", arsort),
        ("ksort", ksort),
        ("krsort", krsort),
    ] {
        registry.function(FunctionEntry::sync(name, ARRAY_SORT_PARAMS, body));
    }
    registry.function(FunctionEntry::sync(
        "natsort",
        ARRAY_NATURAL_PARAMS,
        natsort,
    ));
    registry.function(FunctionEntry::sync(
        "natcasesort",
        ARRAY_NATURAL_PARAMS,
        natcasesort,
    ));
}

fn sort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        Keys::Renumber,
        SortBy::Values,
        Direction::Ascending,
        None,
    )
}

fn rsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        Keys::Renumber,
        SortBy::Values,
        Direction::Descending,
        None,
    )
}

fn asort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        Keys::Preserve,
        SortBy::Values,
        Direction::Ascending,
        None,
    )
}

fn arsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        Keys::Preserve,
        SortBy::Values,
        Direction::Descending,
        None,
    )
}

fn ksort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        Keys::Preserve,
        SortBy::Keys,
        Direction::Ascending,
        None,
    )
}

fn krsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        Keys::Preserve,
        SortBy::Keys,
        Direction::Descending,
        None,
    )
}

fn natsort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        Keys::Preserve,
        SortBy::Values,
        Direction::Ascending,
        Some(SortOrder::Natural { ignore_case: false }),
    )
}

fn natcasesort(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    run(
        context,
        arguments,
        Keys::Preserve,
        SortBy::Values,
        Direction::Ascending,
        Some(SortOrder::Natural { ignore_case: true }),
    )
}

/// Original ordinals break ties even for descending sorts, retaining PHP 8 stability.
fn run(
    context: &mut dyn Context,
    arguments: &[Value],
    keys: Keys,
    sort_by: SortBy,
    direction: Direction,
    fixed: Option<SortOrder>,
) -> NativeResult {
    let order =
        fixed.unwrap_or_else(|| SortOrder::from_flags(args::int(arguments, 1).unwrap_or_default()));
    let target = args::reference(arguments, 0)
        .ok_or_else(|| NativeError::error("Expected array reference"))?;
    let entries: Vec<(ArrayKey, Value)> = {
        let value = target
            .try_borrow()
            .map_err(|_| NativeError::error("Array is already being modified"))?;
        let Value::Array(array) = &*value else {
            return Err(NativeError::error("Expected array"));
        };
        array
            .iter()
            .map(|(key, value)| (key, value.clone()))
            .collect()
    };

    let mut failure = None;
    let indices = super::sort_order::order(entries.len(), |left, right| {
        let (Some(left_entry), Some(right_entry)) = (entries.get(left), entries.get(right)) else {
            return Ok(Ordering::Equal);
        };
        let compared = match sort_by {
            SortBy::Keys => super::ordering::compare_values(
                context,
                &super::key_value(&left_entry.0),
                &super::key_value(&right_entry.0),
                order,
            ),
            SortBy::Values if failure.is_some() && matches!(order, SortOrder::Regular) => {
                compare_after_error(&left_entry.1, &right_entry.1, context.diagnostics())
                    .map(|result| result.cmp(&0))
                    .map_err(NativeError::from)
            }
            SortBy::Values => {
                super::ordering::compare_values(context, &left_entry.1, &right_entry.1, order)
            }
        };
        let order = match compared {
            Ok(order) => order,
            Err(error) => {
                // Zend keeps sorting with an uncomparable result and throws afterward.
                failure.get_or_insert(error);
                Ordering::Greater
            }
        };
        let order = match direction {
            Direction::Ascending => order,
            Direction::Descending => order.reverse(),
        };
        Ok(order.then_with(|| left.cmp(&right)))
    })?;
    let mut result = PhpArray::with_capacity(entries.len());
    for index in indices {
        if let Some((key, value)) = entries.get(index) {
            match keys {
                Keys::Preserve => {
                    result.insert(key.clone(), value.clone());
                }
                Keys::Renumber => {
                    result.push(value.clone())?;
                }
            }
        }
    }
    result.reset();
    let mut target = target
        .try_borrow_mut()
        .map_err(|_| NativeError::error("Array is already being modified"))?;
    *target = Value::from(result);

    match failure {
        Some(error) => Err(error),
        None => Ok(Value::Bool(true)),
    }
}
