//! Sorted set traversal: `array_uintersect([2, 2, 1], [2], 'strcmp')` keeps both 2s.

use std::cmp::Ordering;

use phera_extension_api::{Context, NativeResult, PhpArray, Value};

use super::callback_sets::{Entry, SetComparison};

pub(super) async fn run(
    context: &mut dyn Context,
    first: &PhpArray,
    sorted: &[Vec<Entry>],
    comparison: &mut SetComparison<'_>,
    intersection: bool,
) -> NativeResult {
    if intersection {
        intersect(context, first, sorted, comparison).await
    } else {
        difference(context, first, sorted, comparison).await
    }
}

/// Mirrors php_array_diff's asymmetric scans: values advance persistent cursors;
/// callback keys search each other list from its start for every first-array key.
async fn difference(
    context: &mut dyn Context,
    first: &PhpArray,
    sorted: &[Vec<Entry>],
    comparison: &mut SetComparison<'_>,
) -> NativeResult {
    let mut result = first.clone();
    let mut positions = vec![0; sorted.len()];
    let mut current = 0;
    while let Some(entry) = sorted[0].get(current) {
        let mut order = Ordering::Greater;
        for (index, other) in sorted.iter().enumerate().skip(1) {
            let mut position = positions[index];
            while let Some(candidate) = other.get(position) {
                order = comparison.order(context, entry, candidate).await?;
                let advance = if comparison.keys.is_some() {
                    order != Ordering::Equal
                } else {
                    order == Ordering::Greater
                };
                if !advance {
                    break;
                }
                position += 1;
            }
            if comparison.keys.is_none() {
                positions[index] = position;
            }
            if order == Ordering::Equal {
                if comparison.keys.is_none() {
                    if position < other.len() {
                        positions[index] += 1;
                    }
                    break;
                }
                if comparison.key_only {
                    break;
                }
                if let Some(candidate) = other.get(position) {
                    if comparison.data(context, entry, candidate, true).await? == Ordering::Equal {
                        break;
                    }
                    order = Ordering::Less;
                }
            }
        }
        loop {
            if order == Ordering::Equal {
                result.remove(&sorted[0][current].0);
            }
            current += 1;
            let Some(next) = sorted[0].get(current) else {
                return Ok(Value::from(result));
            };
            if comparison.keys.is_some()
                || comparison
                    .order(context, &sorted[0][current - 1], next)
                    .await?
                    != Ordering::Equal
            {
                break;
            }
        }
    }
    Ok(Value::from(result))
}

/// Intersection consumes a match from each other list, then retains duplicate
/// values in the first list. Rejected values skip directly to the failing list.
async fn intersect(
    context: &mut dyn Context,
    first: &PhpArray,
    sorted: &[Vec<Entry>],
    comparison: &mut SetComparison<'_>,
) -> NativeResult {
    let mut result = first.clone();
    let mut positions = vec![0; sorted.len()];
    let mut current = 0;
    while let Some(entry) = sorted[0].get(current) {
        let mut order = Ordering::Equal;
        let mut failed = 0;
        let mut data_compares_keys = false;
        for (index, other) in sorted.iter().enumerate().skip(1) {
            while let Some(candidate) = other.get(positions[index]) {
                order = comparison
                    .continuing_order(context, entry, candidate, data_compares_keys)
                    .await?;
                if order != Ordering::Greater {
                    break;
                }
                positions[index] += 1;
            }
            let Some(candidate) = other.get(positions[index]) else {
                for remaining in sorted[0].iter().skip(current) {
                    result.remove(&remaining.0);
                }
                return Ok(Value::from(result));
            };
            if order == Ordering::Equal && comparison.keys.is_some() && !comparison.key_only {
                if comparison.data(context, entry, candidate, true).await? != Ordering::Equal {
                    order = Ordering::Greater;
                    data_compares_keys = false;
                } else {
                    data_compares_keys = comparison.data.is_some();
                }
            }
            if order != Ordering::Equal {
                failed = index;
                break;
            }
            positions[index] += 1;
        }
        loop {
            if order != Ordering::Equal {
                result.remove(&sorted[0][current].0);
            }
            current += 1;
            let Some(next) = sorted[0].get(current) else {
                return Ok(Value::from(result));
            };
            if comparison.keys.is_some() {
                break;
            }
            if order != Ordering::Equal {
                if comparison
                    .order(context, next, &sorted[failed][positions[failed]])
                    .await?
                    != Ordering::Less
                {
                    break;
                }
            } else if comparison
                .order(context, &sorted[0][current - 1], next)
                .await?
                != Ordering::Equal
            {
                break;
            }
        }
    }
    Ok(Value::from(result))
}
