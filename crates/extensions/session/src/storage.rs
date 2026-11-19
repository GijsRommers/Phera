//! A session calls `open`, `read`, then `write` or `updateTimestamp`, then `close`.

use phera_extension_api::{Context, NativeError, PhpString, Value};

use crate::{files, settings, state::state, user_handler as user};

pub async fn open(
    context: &mut dyn Context,
    path: &PhpString,
    name: PhpString,
    function: &str,
) -> Result<bool, NativeError> {
    if let Some(object) = user::handler(context) {
        state(context).user_open = true;
        user::boolean(
            context,
            object,
            "open",
            vec![Value::String(path.clone()), name.into()],
            function,
        )
        .await
    } else {
        match files::open(context, path) {
            Ok(()) => Ok(true),
            Err(message) => {
                context.warning(message);
                Ok(false)
            }
        }
    }
}

pub async fn validate(
    context: &mut dyn Context,
    id: &PhpString,
    function: &str,
) -> Result<bool, NativeError> {
    if let Some(object) = user::handler(context) {
        if context.has_method(&object, "validateId") {
            user::boolean(
                context,
                object,
                "validateId",
                vec![id.clone().into()],
                function,
            )
            .await
        } else {
            Ok(true)
        }
    } else {
        let directory = state(context)
            .files
            .as_ref()
            .map(|files| files.directory.clone());
        match directory {
            Some(directory) => directory.known(id).await,
            None => Ok(false),
        }
    }
}

pub fn validates(context: &mut dyn Context) -> bool {
    user::handler(context).is_none_or(|object| context.has_method(&object, "validateId"))
}

pub async fn read(
    context: &mut dyn Context,
    id: &PhpString,
    function: &str,
) -> Result<Option<Vec<u8>>, NativeError> {
    if let Some(object) = user::handler(context) {
        let value = user::call(context, object, "read", vec![id.clone().into()], function).await?;
        Ok(match value {
            Some(Value::String(bytes)) => Some(bytes.as_bytes().to_vec()),
            _ => None,
        })
    } else {
        files::read(context, id, function).await
    }
}

pub fn write_method(context: &mut dyn Context, unchanged: bool) -> &'static str {
    if unchanged
        && user::handler(context)
            .is_some_and(|object| context.has_method(&object, "updateTimestamp"))
    {
        "updateTimestamp"
    } else {
        "write"
    }
}

pub async fn write(
    context: &mut dyn Context,
    id: &PhpString,
    data: Vec<u8>,
    unchanged: bool,
    function: &str,
) -> Result<bool, NativeError> {
    if let Some(object) = user::handler(context) {
        let method = write_method(context, unchanged);
        user::boolean(
            context,
            object,
            method,
            vec![id.clone().into(), data.into()],
            function,
        )
        .await
    } else {
        files::write(context, id, data, unchanged, function).await
    }
}

pub async fn close(context: &mut dyn Context, function: &str) -> Result<(), NativeError> {
    let result = if std::mem::take(&mut state(context).user_open)
        && let Some(object) = user::handler(context)
    {
        user::boolean(context, object, "close", Vec::new(), function)
            .await
            .map(|_| ())
    } else {
        Ok(())
    };
    state(context).files = None;
    result
}

pub async fn destroy(
    context: &mut dyn Context,
    id: &PhpString,
    function: &str,
) -> Result<bool, NativeError> {
    if let Some(object) = user::handler(context) {
        user::boolean(
            context,
            object,
            "destroy",
            vec![id.clone().into()],
            function,
        )
        .await
    } else {
        files::destroy(context, id).await
    }
}

pub async fn collect(
    context: &mut dyn Context,
    function: &str,
) -> Result<Option<i64>, NativeError> {
    let lifetime = settings::integer(context, "gc_maxlifetime");
    if let Some(object) = user::handler(context) {
        let result =
            user::call(context, object, "gc", vec![Value::Int(lifetime)], function).await?;
        match result {
            Some(Value::Int(value)) if value >= 0 => Ok(Some(value)),
            Some(Value::Bool(true)) => Ok(Some(1)),
            _ => Ok(None),
        }
    } else {
        files::collect(context, lifetime).await
    }
}
