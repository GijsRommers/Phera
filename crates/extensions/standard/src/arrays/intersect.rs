//! PHP 8.6 converts entries of the later arrays before the first array.
//!
//! ```php
//! array_intersect(['one' => 1, 'two' => 2], ['2']); // ['two' => 2]
//! ```

use std::collections::HashMap;

use phera_extension_api::{Context, LocalFuture, NativeResult, Value};

use crate::arguments::required_array;

pub(super) fn intersect(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    if arguments.len() < 2 {
        // PHP still uses its sort-based comparison for a single array.
        return super::callback_sets::intersect_strings(context, arguments);
    }
    Box::pin(async move {
        filter(context, &arguments)
            .await
            .map_err(|error| context.native_call_error("array_intersect", &arguments, error))
    })
}

async fn filter(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mut arrays = (0..arguments.len()).map(|index| required_array(arguments, index));
    let Some(first) = arrays.next() else {
        return Ok(Value::from(phera_extension_api::PhpArray::new()));
    };
    let first_later = required_array(arguments, 1);
    let mut result = (**first).clone();
    let mut removed = Vec::new();
    // Argument binding checks every array type before any string conversions.
    if first.is_empty() || arrays.clone().any(|array| array.is_empty()) {
        removed.extend(first.iter().map(|(key, _)| key));
    } else {
        let mut memberships = HashMap::with_capacity(first_later.len());
        for value in first_later.values() {
            memberships.insert(context.stringify(value).await?, 1);
        }
        for (position, array) in arrays.enumerate().skip(1) {
            for value in array.values() {
                let key = context.stringify(value).await?;
                if let Some(count) = memberships.get_mut(&key)
                    && *count == position
                {
                    *count += 1;
                }
            }
        }
        for (key, value) in first.iter() {
            let text = context.stringify(value).await?;
            if memberships.get(&text) != Some(&(arguments.len() - 1)) {
                removed.push(key);
            }
        }
    }
    // Finish all conversions before releasing any entries.
    for key in removed {
        result.remove(&key);
    }
    Ok(Value::from(result))
}
