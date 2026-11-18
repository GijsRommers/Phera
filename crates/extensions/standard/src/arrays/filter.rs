//! `array_filter` and `array_reduce`.

use phera_extension_api::value::cast_to_bool;
use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, PhpArray, Value, args,
};

use super::callback::{check_callback, check_callback_or_null};
use crate::arguments::required_array;
use crate::constants::{ARRAY_FILTER_USE_BOTH, ARRAY_FILTER_USE_KEY};

pub(super) const FUNCTIONS: &[FunctionEntry] = &[ARRAY_FILTER, ARRAY_REDUCE];

/// `array_filter(array $array, ?callable $callback = null, int $mode = 0):
/// array`, the entries the callback accepts, keys kept. Without a callback,
/// the entries whose value is truthy.
///
/// The callback gets the value, the key (`ARRAY_FILTER_USE_KEY`) or both
/// (`ARRAY_FILTER_USE_BOTH`).
///
/// ```php
/// array_filter([0, 2, null, "a"]);                       // [1 => 2, 3 => "a"]
/// array_filter(["a" => 1, "b" => 2], fn($k) => $k != "a", ARRAY_FILTER_USE_KEY); // ["b" => 2]
/// ```
const ARRAY_FILTER: FunctionEntry = FunctionEntry::asynchronous(
    "array_filter",
    &[
        Param::new("array", ParamType::Array),
        Param::new("callback", ParamType::Callable)
            .nullable()
            .optional(),
        Param::new("mode", ParamType::Int).optional(),
    ],
    array_filter,
);

fn array_filter(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let callback = args::value(&arguments, 1);
        check_callback_or_null("array_filter", callback)?;
        let mode = args::int(&arguments, 2).unwrap_or_default();
        let mut result = PhpArray::new();
        for (key, value) in required_array(&arguments, 0).iter() {
            let verdict = if callback.is_null() {
                value.deref().clone()
            } else {
                let parameters = match mode {
                    ARRAY_FILTER_USE_BOTH => vec![value.clone(), Value::from(key.clone())],
                    ARRAY_FILTER_USE_KEY => vec![Value::from(key.clone())],
                    _ => vec![value.clone()],
                };
                context.call(callback, parameters).await?
            };
            if cast_to_bool(&verdict, context.diagnostics()) {
                result.insert(key, value.clone());
            }
        }
        Ok(Value::from(result))
    })
}

/// `array_reduce(array $array, callable $callback, mixed $initial = null):
/// mixed`, folding the values into one with `$callback($carry, $value)`.
///
/// ```php
/// array_reduce([1, 2, 3], fn($carry, $value) => $carry + $value, 10); // 16
/// ```
const ARRAY_REDUCE: FunctionEntry = FunctionEntry::asynchronous(
    "array_reduce",
    &[
        Param::new("array", ParamType::Array),
        Param::new("callback", ParamType::Callable),
        Param::new("initial", ParamType::Mixed).optional(),
    ],
    array_reduce,
);

fn array_reduce(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let callback = &arguments[1];
        check_callback("array_reduce", callback)?;
        let mut carry = args::value(&arguments, 2).clone();
        for value in required_array(&arguments, 0).values() {
            let next = context.call(callback, vec![carry, value.clone()]).await?;
            carry = next.deref().clone();
        }
        Ok(carry)
    })
}
