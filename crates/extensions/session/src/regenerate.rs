//! `session_regenerate_id(true)` deletes the old ID and keeps the live session values.

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Value, args,
};

use crate::{
    headers, lifecycle, settings,
    start::require_id,
    state::{active, after_output, after_start, state},
    storage, user_handler, variables,
};

pub const FUNCTION: FunctionEntry = FunctionEntry::asynchronous(
    "session_regenerate_id",
    &[Param::new("delete_old_session", ParamType::Bool).optional()],
    regenerate,
);

fn regenerate(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        const FUNCTION: &str = "session_regenerate_id";
        if !active(context) {
            let message = after_start(
                context,
                "session_regenerate_id(): Session ID cannot be regenerated when there \
                 is no active session",
            );
            context.warning(message);
            return Ok(Value::Bool(false));
        }
        if context.output_origin().is_some() {
            let message = "session_regenerate_id(): Session ID cannot be regenerated after \
                           headers have already been sent";
            context.warning(after_output(context, message));
            return Ok(Value::Bool(false));
        }

        let delete = args::bool(&arguments, 0).unwrap_or(false);
        let id = require_id(context)?;
        let action = if delete {
            storage::destroy(context, &id, FUNCTION).await
        } else {
            match variables::encode(context, FUNCTION).await {
                Ok(data) => {
                    storage::write(context, &id, data.unwrap_or_default(), false, FUNCTION).await
                }
                Err(error) => Err(error),
            }
        };
        let action = lifecycle::close_after(context, FUNCTION, action).await;
        if !matches!(action, Ok(true)) {
            state(context).active = false;
            action?;
            let message = if delete {
                "Session object destruction failed"
            } else {
                "Session write failed"
            };
            let backend = backend(context);
            context.warning(format!("{FUNCTION}(): {message}. ID: {backend}"));
            return Ok(Value::Bool(false));
        }
        let session = state(context);
        session.before = None;
        session.id = None;
        if let Err(error) = replace_id(context, FUNCTION).await {
            storage::release(context);
            state(context).active = false;
            return Err(error);
        }
        let sent = !settings::boolean(context, "use_cookies") || headers::cookie(context, FUNCTION);
        Ok(Value::Bool(sent))
    })
}

async fn replace_id(context: &mut dyn Context, function: &str) -> Result<(), NativeError> {
    let path = settings::text(context, "save_path");
    let name = settings::text(context, "name");
    if !storage::open(context, &path, name, function).await? {
        return Err(NativeError::error(format!(
            "Failed to open session: {}",
            backend(context)
        )));
    }
    let mut id = user_handler::create(context, function).await?;
    if settings::boolean(context, "use_strict_mode") && storage::validates(context) {
        for _ in 0..3 {
            if !storage::validate(context, &id, function).await? {
                break;
            }
            id = user_handler::create(context, function).await?;
        }
    }
    state(context).id = Some(id.clone());
    if storage::read(context, &id, function).await?.is_none() {
        return Err(NativeError::error(format!(
            "Failed to create(read) session ID: {}",
            backend(context)
        )));
    }
    Ok(())
}

fn backend(context: &mut dyn Context) -> String {
    let module = settings::text(context, "save_handler");
    let path = settings::text(context, "save_path");
    format!(
        "{} (path: {})",
        module.to_string_lossy(),
        path.to_string_lossy()
    )
}
