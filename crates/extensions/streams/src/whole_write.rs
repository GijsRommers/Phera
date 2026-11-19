//! `file_put_contents('report.txt', ['a', 'b'], FILE_APPEND | LOCK_EX)` appends "ab".

use std::io;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Value, args, value::to_php_string,
};
use rustix::fs::{FlockOperation, flock};

use crate::errors::warn_false;
use crate::handles::read_notice;
use crate::io_policy::{CHUNK, IoPolicy};
use crate::request::files;
use crate::resource::stream_argument;
use crate::stream::{Stream, Wrapper};
use crate::transfer::{finish_write, read, write_bytes};
use crate::whole_file::{APPEND, USE_PATH, name};
use crate::wrapper::open;

const LOCK_EX: i64 = 2;

pub fn file_put_contents(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let function = "file_put_contents";
        let name = name(&arguments, function, 3)?;
        let flags = args::int(&arguments, 2).unwrap_or(0);
        let data = args::value(&arguments, 1);
        let source = if matches!(data, Value::Resource(_)) {
            Some(stream_argument(data, function, "#2 ($data)").map_err(|_| {
                NativeError::type_error(
                    "file_put_contents(): supplied resource is not a valid stream resource",
                )
            })?)
        } else {
            None
        };
        let lock = flags & LOCK_EX != 0;
        let mode = if flags & APPEND != 0 {
            b"ab"
        } else if lock {
            if name.windows(3).any(|slice| slice == b"://")
                && !name
                    .get(..7)
                    .is_some_and(|part| part.eq_ignore_ascii_case(b"file://"))
            {
                return warn_false(
                    context,
                    "file_put_contents(): Exclusive locks may only be set for regular files".into(),
                );
            }
            b"cb"
        } else {
            b"wb"
        };
        if flags & crate::whole_file::NO_DEFAULT_CONTEXT == 0 {
            files(context).use_default_context(args::value(&arguments, 3));
        }
        let Some(mut stream) = open(context, function, name, mode, flags & USE_PATH != 0).await?
        else {
            return Ok(Value::Bool(false));
        };
        files(context).claim_id();
        if lock {
            let (locked, restored) = IoPolicy::Blocking
                .run(move || {
                    let locked = stream
                        .descriptor()
                        .is_some_and(|file| flock(file, FlockOperation::LockExclusive).is_ok());
                    if locked && mode == b"cb" {
                        let _ = stream.truncate(0);
                    }
                    (locked, stream)
                })
                .await?;
            stream = restored;
            if !locked {
                return warn_false(
                    context,
                    "file_put_contents(): Exclusive locks are not supported for this stream".into(),
                );
            }
        }
        let bytes = if let Some(resource) = source {
            match read(resource, None).await? {
                Ok(bytes) => bytes,
                Err(error) => {
                    read_notice(context, function, &error, CHUNK as u64);
                    return Ok(Value::Bool(false));
                }
            }
        } else {
            let Some(bytes) = join_data(context, data)? else {
                return Ok(Value::Bool(false));
            };
            bytes
        };
        let size = bytes.len();
        let wrapper = stream.wrapper;
        let (bytes, result) = write_owned(stream, bytes).await?;
        if let Ok(count) = result
            && count != size
        {
            return warn_false(
                context,
                format!(
                    "{}(): Only {count} of {size} bytes written, possibly out of free disk space",
                    function
                ),
            );
        }
        finish_write(context, wrapper, &bytes, function, result)
    })
}

/// Appends the bytes of `error_log($message, 3, $destination)` using the
/// request's stream wrappers and working directory.
pub async fn append_log(
    context: &mut dyn Context,
    destination: &[u8],
    message: &[u8],
) -> NativeResult {
    let function = "error_log";
    let Some(stream) = open(context, function, destination, b"a", false).await? else {
        return Ok(Value::Bool(false));
    };
    let wrapper = stream.wrapper;
    let (bytes, written) = write_owned(stream, message.to_vec()).await?;
    // PHP's read-only memory wrappers reject this write silently.
    if matches!(wrapper, Wrapper::Input | Wrapper::Data) && written.is_err() {
        return Ok(Value::Bool(false));
    }
    match finish_write(context, wrapper, &bytes, function, written)? {
        Value::Int(count) => Ok(Value::Bool(count as usize == bytes.len())),
        _ => Ok(Value::Bool(false)),
    }
}

async fn write_owned(
    mut stream: Stream,
    bytes: Vec<u8>,
) -> Result<(Vec<u8>, io::Result<usize>), NativeError> {
    if stream.wrapper == Wrapper::Output {
        let result = stream.write(&bytes);
        return Ok((bytes, result));
    }
    stream
        .write_policy(bytes.len() as u64)
        .run(move || {
            let result = write_bytes(&mut stream, &bytes);
            (bytes, result)
        })
        .await
}

fn join_data(context: &mut dyn Context, data: &Value) -> Result<Option<Vec<u8>>, NativeError> {
    let mut bytes = Vec::new();
    match data {
        Value::Array(parts) => {
            for (_, field) in parts.iter() {
                bytes.extend_from_slice(to_php_string(field, context.diagnostics())?.as_bytes());
            }
        }
        Value::Object(_) => return Ok(None),
        value => bytes.extend_from_slice(to_php_string(value, context.diagnostics())?.as_bytes()),
    }
    Ok(Some(bytes))
}
