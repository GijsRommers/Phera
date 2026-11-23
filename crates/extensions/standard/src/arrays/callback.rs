//! What the callback functions share: checking a callback argument before
//! anything else, and calling a comparison callback.

use std::cmp::Ordering;

use phera_extension_api::value::to_int;
use phera_extension_api::{ArrayKey, Context, NativeError, Value};

/// Checks argument #2, `$callback`, of a function that takes one, even when
/// the array is empty and it would never be called.
///
/// ```php
/// array_filter([], 42); // TypeError: ... must be a valid callback or null, no array or string given
/// ```
pub(super) fn check_callback(function: &str, callable: &Value) -> Result<(), NativeError> {
    check(function, "#2 ($callback)", callable, NullCallback::Rejected)
}

/// The same for `array_filter`, whose callback may be null.
pub(super) fn check_callback_or_null(function: &str, callable: &Value) -> Result<(), NativeError> {
    check(function, "#2 ($callback)", callable, NullCallback::Allowed)
}

/// The same for the callbacks at the end of `array_udiff` and its
/// relatives, which PHP names by number only.
pub(super) fn check_trailing_callback(
    function: &str,
    number: usize,
    callable: &Value,
) -> Result<(), NativeError> {
    check(
        function,
        &format!("#{number}"),
        callable,
        NullCallback::Rejected,
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NullCallback {
    Allowed,
    Rejected,
}

/// Whether a function or method of that name exists is for the VM to say
/// when it calls it; this checks only the shape PHP checks first.
fn check(
    function: &str,
    argument: &str,
    callable: &Value,
    null: NullCallback,
) -> Result<(), NativeError> {
    let reason = match &*callable.deref() {
        Value::Null if null == NullCallback::Allowed => return Ok(()),
        Value::InternedStr(_) | Value::String(_) | Value::Object(_) => return Ok(()),
        Value::Array(array) => {
            let first = array.get(&ArrayKey::Int(0));
            let second = array.get(&ArrayKey::Int(1));
            if array.len() != 2 {
                "array callback must have exactly two members"
            } else if first.is_none() || second.is_none() {
                "array callback has to contain indices 0 and 1"
            } else if !first.is_some_and(|value| value.is_string() || value.is_object()) {
                "first array member is not a valid class name or object"
            } else if !second.is_some_and(Value::is_string) {
                "second array member is not a valid method"
            } else {
                return Ok(());
            }
        }
        _ => "no array or string given",
    };
    let or_null = if null == NullCallback::Allowed {
        " or null"
    } else {
        ""
    };
    Err(NativeError::type_error(format!(
        "{function}(): Argument {argument} must be a valid callback{or_null}, {reason}"
    )))
}

/// What a comparison callback's `bool` result means.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BoolResult {
    /// PHP 8 compatibility for old `return $a > $b;` callbacks: `true` is
    /// "greater", but `false` may be "less" or "equal", so PHP asks again
    /// with the arguments swapped. Deprecated, once per call.
    AskAgain,
    /// Just 0 or 1, like any other value converted to int.
    Number,
}

/// A PHP comparison callback, as `usort` and `array_udiff` call it
/// (`php_array_user_compare_unstable` in ext/standard/array.c).
pub(super) struct UserComparison<'a> {
    /// The PHP function, for the deprecation message.
    pub function: &'a str,
    /// Whether the bool deprecation was already raised.
    pub deprecated: bool,
}

impl UserComparison<'_> {
    pub async fn compare(
        &mut self,
        context: &mut dyn Context,
        callback: &Value,
        left: Value,
        right: Value,
        bools: BoolResult,
    ) -> Result<Ordering, NativeError> {
        let result = context
            .call(callback, vec![left.clone(), right.clone()])
            .await?;
        let result = result.deref().clone();
        if bools == BoolResult::AskAgain
            && let Value::Bool(greater) = result
        {
            if !self.deprecated {
                context.deprecated(format!(
                    "{}(): Returning bool from comparison function is deprecated, \
                     return an integer less than, equal to, or greater than zero",
                    self.function
                ));
                self.deprecated = true;
            }
            if !greater {
                let swapped = context.call(callback, vec![right, left]).await?;
                return Ok(to_int(&swapped, context.diagnostics())?.cmp(&0).reverse());
            }
        }
        Ok(to_int(&result, context.diagnostics())?.cmp(&0))
    }
}
