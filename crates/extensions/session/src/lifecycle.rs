//! `session_abort()` closes the storage and leaves the in-memory values intact.

use phera_extension_api::{Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Value};

use crate::{
    settings,
    start::{self, require_id},
    state::{active, state},
    storage, user_handler, variables,
};

pub const FUNCTIONS: &[FunctionEntry] = &[
    FunctionEntry::asynchronous("session_write_close", &[], |context, _| {
        Box::pin(close_function(context, "session_write_close", Close::Write))
    }),
    FunctionEntry::asynchronous("session_commit", &[], |context, _| {
        Box::pin(close_function(context, "session_commit", Close::Write))
    }),
    FunctionEntry::asynchronous("session_abort", &[], |context, _| {
        Box::pin(close_function(context, "session_abort", Close::Discard))
    }),
    FunctionEntry::asynchronous("session_destroy", &[], |context, _| {
        Box::pin(async move { Ok(Value::Bool(destroy(context, "session_destroy").await?)) })
    }),
    FunctionEntry::asynchronous("session_reset", &[], |context, _| {
        Box::pin(async move {
            if !active(context) {
                return Ok(Value::Bool(false));
            }
            start::initialize(context, "session_reset", false, true).await?;
            Ok(Value::Bool(true))
        })
    }),
    FunctionEntry::asynchronous("session_gc", &[], collect),
];

pub enum Close {
    Write,
    Discard,
}

async fn close_function(context: &mut dyn Context, function: &str, mode: Close) -> NativeResult {
    Ok(Value::Bool(finish(context, mode, function).await?))
}

pub fn finalize(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        finish(context, Close::Write, "PHP Request Shutdown").await?;
        Ok(Value::Null)
    })
}

pub async fn finish(
    context: &mut dyn Context,
    mode: Close,
    function: &str,
) -> Result<bool, NativeError> {
    if !active(context) {
        return Ok(false);
    }
    let result = match mode {
        Close::Write => flush(context, function).await,
        Close::Discard => Ok(()),
    };
    let close = storage::close(context, function).await;
    state(context).active = false;
    combine(context, result, close)?;
    Ok(true)
}

async fn flush(context: &mut dyn Context, function: &str) -> Result<(), NativeError> {
    if !variables::current(context)?.is_some_and(|value| value.is_array()) {
        return Ok(());
    }
    let id = require_id(context)?;
    let encoded = variables::encode(context, function).await?;
    let unchanged = settings::boolean(context, "lazy_write")
        && encoded
            .as_ref()
            .is_some_and(|data| state(context).before.as_ref() == Some(data));
    let method = storage::write_method(context, unchanged);
    if !storage::write(
        context,
        &id,
        encoded.unwrap_or_default(),
        unchanged,
        function,
    )
    .await?
    {
        let path = settings::text(context, "save_path");
        let path = path.to_string_lossy();
        let message = match user_handler::handler(context) {
            Some(object) => format!(
                "Failed to write session data using user defined save handler. \
                 (session.save_path: {path}, handler: {}::{method})",
                object.class.name()
            ),
            None => format!(
                "Failed to write session data (files). Please verify that the current setting of \
                 session.save_path is correct ({path})"
            ),
        };
        context.warning(format!("{function}(): {message}"));
    }
    Ok(())
}

pub async fn destroy(context: &mut dyn Context, function: &str) -> Result<bool, NativeError> {
    if !active(context) {
        context.warning(format!(
            "{function}(): Trying to destroy uninitialized session"
        ));
        return Ok(false);
    }
    let result = match require_id(context) {
        Ok(id) => storage::destroy(context, &id, function).await,
        Err(error) => Err(error),
    };
    if matches!(result, Ok(false)) {
        context.warning(format!("{function}(): Session object destruction failed"));
    }
    let close = storage::close(context, function).await;
    let session = state(context);
    session.active = false;
    session.id = None;
    session.started = None;
    session.variables = None;
    session.before = None;
    combine(context, result, close)
}

pub fn combine<T>(
    context: &mut dyn Context,
    result: Result<T, NativeError>,
    close: Result<(), NativeError>,
) -> Result<T, NativeError> {
    match (result, close) {
        (Err(previous), Err(error)) => Err(context.chain_exception(error, previous)),
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(value), Ok(())) => Ok(value),
    }
}

fn collect(context: &mut dyn Context, _: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "session_gc";
        if !active(context) {
            context.warning(format!(
                "{function}(): Session cannot be garbage collected when there is no active session"
            ));
            return Ok(Value::Bool(false));
        }
        Ok(storage::collect(context, function)
            .await?
            .map_or(Value::Bool(false), Value::Int))
    })
}
