//! Searching with a callback, new in PHP 8.4: `array_find`,
//! `array_find_key`, `array_any` and `array_all`. Each stops at the first
//! entry that decides the answer.
//!
//! ```php
//! array_find([1, 2, 3], fn($value, $key) => $value > 1); // 2
//! array_all([1, 2, 3], fn($value) => $value > 0);        // true
//! ```

use phera_extension_api::value::cast_to_bool;
use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeResult, Param, ParamType, Value,
};

use super::callback::check_callback;
use crate::arguments::required_array;

pub(super) const FUNCTIONS: &[FunctionEntry] = &[ARRAY_FIND, ARRAY_FIND_KEY, ARRAY_ANY, ARRAY_ALL];

/// What one of the four functions looks for and returns.
#[derive(Clone, Copy)]
enum Search {
    /// `array_find`: the first value the callback accepts, or null.
    Value,
    /// `array_find_key`: its key, or null.
    Key,
    /// `array_any`: whether there is one.
    Any,
    /// `array_all`: whether the callback accepts every entry.
    All,
}

/// `array_find(array $array, callable $callback): mixed`; the callback gets
/// the value and the key.
const ARRAY_FIND: FunctionEntry = FunctionEntry::asynchronous(
    "array_find",
    &[
        Param::new("array", ParamType::Array),
        Param::new("callback", ParamType::Callable),
    ],
    array_find,
);

fn array_find(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(search(context, arguments, "array_find", Search::Value))
}

/// `array_find_key(array $array, callable $callback): mixed`
const ARRAY_FIND_KEY: FunctionEntry = FunctionEntry::asynchronous(
    "array_find_key",
    &[
        Param::new("array", ParamType::Array),
        Param::new("callback", ParamType::Callable),
    ],
    array_find_key,
);

fn array_find_key(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(search(context, arguments, "array_find_key", Search::Key))
}

/// `array_any(array $array, callable $callback): bool`
const ARRAY_ANY: FunctionEntry = FunctionEntry::asynchronous(
    "array_any",
    &[
        Param::new("array", ParamType::Array),
        Param::new("callback", ParamType::Callable),
    ],
    array_any,
);

fn array_any(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(search(context, arguments, "array_any", Search::Any))
}

/// `array_all(array $array, callable $callback): bool`
const ARRAY_ALL: FunctionEntry = FunctionEntry::asynchronous(
    "array_all",
    &[
        Param::new("array", ParamType::Array),
        Param::new("callback", ParamType::Callable),
    ],
    array_all,
);

fn array_all(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(search(context, arguments, "array_all", Search::All))
}

async fn search(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
    search: Search,
) -> NativeResult {
    let callback = &arguments[1];
    check_callback(function, callback)?;
    for (key, value) in required_array(&arguments, 0).iter() {
        let answer = context
            .call(callback, vec![value.clone(), Value::from(key.clone())])
            .await?;
        let accepted = cast_to_bool(&answer, context.diagnostics());
        match search {
            Search::All if !accepted => return Ok(Value::Bool(false)),
            Search::All => {}
            _ if !accepted => {}
            Search::Value => return Ok(value.deref().clone()),
            Search::Key => return Ok(Value::from(key)),
            Search::Any => return Ok(Value::Bool(true)),
        }
    }
    Ok(match search {
        Search::Value | Search::Key => Value::Null,
        Search::Any => Value::Bool(false),
        Search::All => Value::Bool(true),
    })
}
