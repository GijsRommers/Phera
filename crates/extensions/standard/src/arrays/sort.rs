//! Stable array sorting with PHP-compatible comparison traversal.

use std::cmp::Ordering;

use phera_extension_api::{
    ArrayKey, Context, FunctionEntry, NativeResult, Param, ParamType, PhpArray, Registry, Value,
    args,
};

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
    super::stack::mutate(arguments, |array| {
        let entries: Vec<(ArrayKey, Value)> = array
            .iter()
            .map(|(key, value)| (key, value.clone()))
            .collect();
        let indices = super::sort_order::order(entries.len(), |left, right| {
            let (Some(left_entry), Some(right_entry)) = (entries.get(left), entries.get(right))
            else {
                return Ok(Ordering::Equal);
            };
            let order = if matches!(sort_by, SortBy::Keys) {
                super::ordering::compare_values(
                    context,
                    &super::key_value(&left_entry.0),
                    &super::key_value(&right_entry.0),
                    order,
                )?
            } else {
                super::ordering::compare_values(context, &left_entry.1, &right_entry.1, order)?
            };
            let order = if matches!(direction, Direction::Descending) {
                order.reverse()
            } else {
                order
            };
            Ok(order.then_with(|| left.cmp(&right)))
        })?;
        let mut result = PhpArray::new();
        for index in indices {
            if let Some((key, value)) = entries.get(index) {
                if matches!(keys, Keys::Preserve) {
                    result.insert(key.clone(), value.clone());
                } else {
                    result.push(value.clone())?;
                }
            }
        }
        result.reset();
        *array = result;
        Ok(Value::Bool(true))
    })
}
