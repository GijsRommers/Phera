//! Callback argument checks: `array_filter([], 42)` throws before iteration.

use std::cmp::Ordering;

use phera_extension_api::value::to_int;
use phera_extension_api::{ArrayKey, Context, NativeError, Value};

/// Resolution of function names and methods requires a runtime API. Check the
/// structural errors locally, including on empty arrays where no call occurs.
pub(super) fn validate(
    name: &str,
    number: usize,
    named: bool,
    callable: &Value,
    nullable: bool,
) -> Result<(), NativeError> {
    let reason = match &*callable.deref() {
        Value::Null if nullable => return Ok(()),
        Value::String(_) | Value::Object(_) => return Ok(()),
        Value::Array(array) => {
            if array.len() != 2 {
                "array callback must have exactly two members"
            } else if !array.contains_key(&ArrayKey::Int(0))
                || !array.contains_key(&ArrayKey::Int(1))
            {
                "array callback has to contain indices 0 and 1"
            } else if !array
                .get(&ArrayKey::Int(0))
                .is_some_and(|value| matches!(&*value.deref(), Value::String(_) | Value::Object(_)))
            {
                "first array member is not a valid class name or object"
            } else if !array
                .get(&ArrayKey::Int(1))
                .is_some_and(|value| value.deref().is_string())
            {
                "second array member is not a valid method"
            } else {
                return Ok(());
            }
        }
        _ => "no array or string given",
    };
    let parameter = if named { " ($callback)" } else { "" };
    let null = if nullable { " or null" } else { "" };
    Err(NativeError::type_error(format!(
        "{name}(): Argument #{number}{parameter} must be a valid callback{null}, {reason}"
    )))
}

/// PHP warns once per sort/set operation and retries false with swapped operands.
pub(super) struct Comparison<'a> {
    pub name: &'a str,
    pub deprecated: bool,
}

impl Comparison<'_> {
    pub async fn compare(
        &mut self,
        context: &mut dyn Context,
        callback: &Value,
        left: Value,
        right: Value,
        boolean_retry: bool,
    ) -> Result<Ordering, NativeError> {
        let result = context
            .call(callback, vec![left.clone(), right.clone()])
            .await?;
        let result = result.deref().clone();
        if boolean_retry && let Value::Bool(boolean) = result {
            if !self.deprecated {
                context.deprecated(format!(
                    "{}(): Returning bool from comparison function is deprecated, \
                     return an integer less than, equal to, or greater than zero",
                    self.name
                ));
                self.deprecated = true;
            }
            if !boolean {
                let swapped = context.call(callback, vec![right, left]).await?;
                return Ok(to_int(&swapped, context.diagnostics())?.cmp(&0).reverse());
            }
        }
        Ok(to_int(&result, context.diagnostics())?.cmp(&0))
    }
}
