//! Register PHP SAX callbacks while the parser is idle.
//!
//! ```php
//! xml_set_character_data_handler($parser, [$feed, 'text']);
//! ```

use phera_extension_api::{Context, NativeError, NativeResult, Value, args};

use crate::bindings::{State, receiver};
use crate::events::Handler;

pub fn elements(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let (_, state) = receiver(arguments)?;
    let function = "xml_set_element_handler";
    let start = handler(context, arguments, 1, function, "start_handler")?;
    let end = handler(context, arguments, 2, function, "end_handler")?;
    set(&state, Handler::Start, start)?;
    set(&state, Handler::End, end)?;
    Ok(Value::Bool(true))
}

pub fn register(
    context: &mut dyn Context,
    arguments: &[Value],
    which: Handler,
    function: &str,
) -> NativeResult {
    let (_, state) = receiver(arguments)?;
    let callback = handler(context, arguments, 1, function, "handler")?;
    set(&state, which, callback)?;
    Ok(Value::Bool(true))
}

fn handler(
    context: &mut dyn Context,
    arguments: &[Value],
    index: usize,
    function: &str,
    parameter: &str,
) -> Result<Option<Value>, NativeError> {
    let value = args::value(arguments, index);
    if value.is_null() {
        return Ok(None);
    }
    if !matches!(
        value,
        Value::InternedStr(_) | Value::String(_) | Value::Array(_) | Value::Object(_)
    ) {
        return Err(NativeError::type_error(format!(
            "{function}(): Argument #{} (${parameter}) must be of type callable|string|null",
            index + 1,
        )));
    }
    if context
        .validate_callback(function, index + 1, parameter, value)
        .is_err()
    {
        if let Some(text) = value.as_str_bytes() {
            context.deprecated(format!(
                "{function}(): Passing non-callable strings is deprecated since 8.4"
            ));
            if text.is_empty() {
                return Ok(None);
            }
            return Err(NativeError::value_error(format!(
                "{function}(): Argument #{} (${parameter}) an object must be set \
                via xml_set_object() to be able to lookup method",
                index + 1,
            )));
        }
        let (index, parameter) = if function == "xml_set_element_handler" {
            (2, "start_handler")
        } else {
            (index + 1, parameter)
        };
        return Err(NativeError::type_error(format!(
            "{function}(): Argument #{index} (${parameter}) must be of type callable|string|null"
        )));
    }
    Ok(Some(value.clone()))
}

fn set(state: &State, which: Handler, callback: Option<Value>) -> Result<(), NativeError> {
    if state.parsing.get() {
        return Err(NativeError::error("Cannot change handlers while parsing"));
    }
    let mut handlers = state.handlers.borrow_mut();
    if let Some(callback) = callback {
        handlers.insert(which, callback);
    } else {
        handlers.remove(&which);
    }
    Ok(())
}
