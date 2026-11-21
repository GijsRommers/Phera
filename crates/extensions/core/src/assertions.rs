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
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, prepare_single_argument,
};

const PARAMS: &[Param] = &[
    Param::new("assertion", ParamType::Mixed).arity_checked_by_handler(),
    Param::new("description", ParamType::Mixed)
        .optional()
        .nullable(),
];
const DESCRIPTION: Param =
    Param::new("description", ParamType::ClassUnion("Throwable|string")).nullable();

pub(crate) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::asynchronous(
        "assert",
        PARAMS,
        assert_with_object,
    ));
    registry.register_sync_selector("assert", |slots| {
        (!matches!(slots, [_, Some(Value::Object(object))]
            if object.class.is_instance_of("Stringable")
                && !object.class.is_instance_of("Throwable")))
        .then_some(assert)
    });
}

fn assert_with_object(
    context: &mut dyn Context,
    mut arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        if !enabled(context) {
            return Ok(Value::Bool(true));
        }
        if !context.strict_types()
            && let [_, description] = arguments.as_mut_slice()
            && matches!(description, Value::Object(object)
                if object.class.is_instance_of("Stringable")
                    && !object.class.is_instance_of("Throwable"))
        {
            *description = Value::String(context.stringify(description).await?);
        }
        check(context, &arguments)
    })
}

fn assert(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    if matches!(arguments, [condition] | [condition, Value::String(_) | Value::Null] if to_bool(condition))
    {
        return Ok(Value::Bool(true));
    }
    if !enabled(context) {
        return Ok(Value::Bool(true));
    }
    check(context, arguments)
}

fn enabled(context: &dyn Context) -> bool {
    context
        .ini_get(b"zend.assertions")
        .is_none_or(|value| phera_extension_api::ini::parse_quantity(value.as_bytes()).0 > 0)
        && context
            .ini_get(b"assert.active")
            .is_none_or(|value| phera_extension_api::ini::boolean(value.as_bytes()))
}

fn check(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (condition, description) = match arguments {
        [condition] => (condition, None),
        [condition, description] => {
            let mut description = description.clone();
            prepare_single_argument("assert", 1, &DESCRIPTION, &mut description, context)?;
            (condition, Some(description))
        }
        [] => {
            return Err(NativeError::argument_count_error(
                "assert() expects at least 1 argument, 0 given",
            ));
        }
        _ => {
            return Err(NativeError::argument_count_error(format!(
                "assert() expects at most 2 arguments, {} given",
                arguments.len()
            )));
        }
    };
    if to_bool(condition) {
        return Ok(Value::Bool(true));
    }
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
