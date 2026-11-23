//! Whole-file calls drive PHP stages in `php://filter` through an owned resource.

use phera_extension_api::{Context, NativeResult, Value};

use crate::{handles, transfer, user_file};

pub(crate) async fn contents(
    context: &mut dyn Context,
    stream: Value,
    function: &str,
    offset: i64,
    length: Option<u64>,
) -> NativeResult {
    let Value::Resource(resource) = &stream else {
        return Ok(stream);
    };
    let result = async {
        if offset != 0 {
            let origin = if offset < 0 { 2 } else { 0 };
            let seek = handles::seek_arguments(
                context,
                vec![stream.clone(), Value::Int(offset), Value::Int(origin)],
                function,
            )
            .await?;
            if !matches!(seek, Value::Int(0)) {
                context.warning(format!(
                    "{function}(): Failed to seek to position {offset} in the stream"
                ));
                return Ok(Value::Bool(false));
            }
        }
        if length.is_none()
            && let Some(file) = user_file::resource_file(resource)
        {
            file.stat(context, function).await?;
        }
        match transfer::read_context(context, resource, function, length, false).await? {
            Ok(bytes) => Ok(Value::from(bytes)),
            Err(error) => {
                handles::read_notice(context, function, &error, handles::read_amount(length));
                Ok(Value::Bool(false))
            }
        }
    }
    .await;
    close_after(context, stream, function, result).await
}

pub(crate) async fn passthru(
    context: &mut dyn Context,
    stream: Value,
    function: &str,
) -> NativeResult {
    if !matches!(stream, Value::Resource(_)) {
        return Ok(stream);
    }
    let result = transfer::fpassthru_arguments(context, vec![stream.clone()], function).await;
    close_after(context, stream, function, result).await
}

pub(crate) async fn close_after(
    context: &mut dyn Context,
    stream: Value,
    function: &str,
    result: NativeResult,
) -> NativeResult {
    let closed = handles::close_arguments(context, vec![stream], function).await;
    let value = result?;
    closed?;
    Ok(value)
}
