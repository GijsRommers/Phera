//! `assert()`: stopping when something the code relies on is not true.
//!
//! ```php
//! assert($order->total >= 0);                    // AssertionError: assert($order->total >= 0)
//! assert($user !== null, 'no user in session');  // AssertionError: no user in session
//! assert($ok, new LogicException('broken'));     // throws the LogicException itself
//! ```
//!
//! The compiler adds the condition's own text as the description when the
//! call gives none, which is how the first form gets its message. Called
//! in another way (`call_user_func('assert', false)`) there is no text, and
//! the message is empty, as in PHP.

use phera_extension_api::value::to_bool;
use phera_extension_api::{
    Context, FunctionEntry, NativeError, NativeResult, Param, ParamType, Registry, Value, args,
};

const PARAMS: &[Param] = &[
    Param::new("assertion", ParamType::Mixed),
    Param::new("description", ParamType::Mixed)
        .optional()
        .nullable(),
];

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::sync("assert", PARAMS, assert));
}

fn assert(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    if context
        .ini_get(b"zend.assertions")
        .is_some_and(|value| phera_extension_api::ini::parse_quantity(value.as_bytes()).0 <= 0)
        || context
            .ini_get(b"assert.active")
            .is_some_and(|value| !phera_extension_api::ini::boolean(value.as_bytes()))
        || to_bool(args::value(arguments, 0))
    {
        return Ok(Value::Bool(true));
    }
    let description = arguments.get(1).map(|value| value.deref().clone());
    if !matches!(description, Some(Value::Object(_)))
        && context
            .ini_get(b"assert.exception")
            .is_some_and(|value| !phera_extension_api::ini::boolean(value.as_bytes()))
    {
        if context
            .ini_get(b"assert.warning")
            .is_none_or(|value| phera_extension_api::ini::boolean(value.as_bytes()))
        {
            let text = match description {
                Some(Value::String(text)) => text.to_string_lossy(),
                _ => "Assertion".to_owned(),
            };
            context.warning(format!("assert(): {text} failed"));
        }
        return Ok(Value::Bool(false));
    }
    Err(match description {
        // A ready-made exception is thrown as it is.
        Some(Value::Object(exception)) => context.throw_object(exception),
        Some(Value::String(text)) => failure(text.to_string_lossy()),
        // No description at all: PHP's message is empty.
        _ => failure(String::new()),
    })
}

/// PHP gives an `AssertionError` raised by `assert()` the code 1.
fn failure(message: String) -> NativeError {
    NativeError::throw_with_code("AssertionError", message, 1)
}
