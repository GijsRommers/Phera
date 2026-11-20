//! `stream_get_line($stream, 8192, "\\r\\n")` returns the record without its delimiter.

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Registry, Value, args,
};

use crate::handles::{STREAM, read_notice};
use crate::io_policy::CHUNK;
use crate::resource::stream_arg;

pub(super) fn register(registry: &mut Registry) {
    registry.function(FunctionEntry::asynchronous(
        "stream_get_line",
        const {
            &[
                STREAM,
                Param::new("length", ParamType::Int),
                Param::new("ending", ParamType::String).with_default(ConstantValue::String("")),
            ]
        },
        get_line,
    ));
}

fn get_line(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "stream_get_line";
        let resource = stream_arg(&arguments, function)?;
        let length = args::int(&arguments, 1).unwrap_or_default();
        if length < 0 {
            return Err(NativeError::value_error(
                "stream_get_line(): Argument #2 ($length) must be greater than or equal to 0",
            ));
        }
        let limit = if length == 0 {
            CHUNK as u64
        } else {
            length as u64
        };
        let ending = args::bytes(&arguments, 2);
        let result = if let Some(io) = super::Io::for_resource(resource) {
            io.read(Some(limit), Some(ending)).await
        } else if ending.is_empty() {
            crate::transfer::read_context(context, resource, function, Some(limit), false).await?
        } else {
            let mut record = Vec::new();
            while (record.len() as u64) < limit {
                match crate::transfer::read_context(context, resource, function, Some(1), false)
                    .await?
                {
                    Ok(bytes) if bytes.is_empty() => break,
                    Ok(bytes) => record.extend_from_slice(&bytes),
                    Err(error) => {
                        read_notice(context, function, &error, limit);
                        return Ok(Value::Bool(false));
                    }
                }
                if record.ends_with(ending) {
                    break;
                }
            }
            Ok(record)
        };
        Ok(match result {
            Ok(bytes) if bytes.is_empty() => Value::Bool(false),
            Ok(mut bytes) => {
                if bytes.ends_with(ending) {
                    bytes.truncate(bytes.len() - ending.len());
                }
                Value::from(bytes)
            }
            Err(error) => {
                read_notice(context, function, &error, limit);
                Value::Bool(false)
            }
        })
    })
}
