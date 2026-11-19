//! `$value =& $_SESSION['key']` aliases an entry in the live session array.

use std::{cell::RefCell, rc::Rc};

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, PhpArray,
    Value, args,
};

use phera_ext_serialize::Decoded;

use crate::{
    codec::Format,
    lifecycle,
    state::{active, state},
};

pub const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::asynchronous("session_encode", &[], encode_function),
    FunctionEntry::asynchronous(
        "session_decode",
        &[Param::new("data", ParamType::String)],
        decode_function,
    ),
    FunctionEntry::sync("session_unset", &[], unset),
];

pub fn publish(context: &mut dyn Context, value: Value) {
    let reference = Value::Reference(Rc::new(RefCell::new(value)));
    state(context).variables = Some(reference.clone());
    context.replace_request_global("_SESSION", reference);
}

pub fn clear(context: &mut dyn Context) {
    publish(context, Value::from(PhpArray::new()));
}

fn unset(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    if !active(context) {
        return Ok(Value::Bool(false));
    }
    if let Some(Value::Reference(reference)) = &state(context).variables
        && let Ok(mut value) = reference.try_borrow_mut()
        && value.is_array()
    {
        *value = Value::from(PhpArray::new());
    }
    Ok(Value::Bool(true))
}

pub fn current(context: &mut dyn Context) -> Result<Option<Value>, NativeError> {
    state(context)
        .variables
        .as_ref()
        .map(|value| Ok(value.try_deref()?.clone()))
        .transpose()
}

pub async fn encode(
    context: &mut dyn Context,
    function: &str,
) -> Result<Option<Vec<u8>>, NativeError> {
    if let Some(value) = current(context)?.filter(Value::is_array) {
        return Format::current(context)
            .encode(context, value, function)
            .await;
    }
    context.warning(format!("{function}(): Cannot encode non-existent session"));
    Ok(None)
}

pub async fn decode(
    context: &mut dyn Context,
    data: &[u8],
    function: &str,
) -> Result<Decoded, NativeError> {
    let format = Format::current(context);
    let publish = match format {
        Format::Array => replace_array,
        Format::Text | Format::Binary => merge_entries,
    };
    let result = format.decode(context, data, function, publish).await?;
    if matches!(result, Decoded::Invalid) {
        let result = lifecycle::destroy(context, function).await;
        clear(context);
        context.warning(format!(
            "{function}(): Failed to decode session object. Session has been destroyed"
        ));
        result?;
    }
    Ok(result)
}

fn replace_array(context: &mut dyn Context, value: Value) -> Result<(), NativeError> {
    if matches!(value, Value::Null) {
        clear(context);
    } else {
        publish(context, value);
    }
    Ok(())
}

fn merge_entries(context: &mut dyn Context, decoded: Value) -> Result<(), NativeError> {
    if let Value::Array(entries) = decoded
        && let Some(Value::Reference(reference)) = &state(context).variables
    {
        let mut value = reference
            .try_borrow_mut()
            .map_err(|_| NativeError::error("Cannot update borrowed session data"))?;
        if let Value::Array(current) = &mut *value {
            let current = Rc::make_mut(current);
            for (key, value) in entries.iter() {
                current.insert(key, value.clone());
            }
        }
    }
    Ok(())
}

fn encode_function(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        Ok(encode(context, "session_encode")
            .await?
            .map_or(Value::Bool(false), Value::from))
    })
}

fn decode_function(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "session_decode";
        if !active(context) {
            context.warning(format!(
                "{function}(): Session data cannot be decoded when there is no active session"
            ));
            return Ok(Value::Bool(false));
        }
        match decode(context, args::bytes(&arguments, 0), function).await? {
            Decoded::Complete => Ok(Value::Bool(true)),
            Decoded::Invalid => Ok(Value::Bool(false)),
            Decoded::HookFailed(error) => Err(error),
        }
    })
}
