//! `file_put_contents('report.txt', ['a', 'b'], FILE_APPEND | LOCK_EX)` appends "ab".

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Value, args, value::to_php_string,
};
use rustix::fs::{FlockOperation, flock};

use crate::errors::warn_false;
use crate::handles::read_notice;
use crate::io_policy::{CHUNK, IoPolicy};
use crate::resource::stream_argument;
use crate::stream::Wrapper;
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
            Some(stream_argument(data, function, "#2 ($data)")?)
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
        let Some(mut stream) = open(context, function, name, mode, flags & USE_PATH != 0).await?
        else {
            return Ok(Value::Bool(false));
        };
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
        let policy = stream.write_policy(size as u64);
        let (bytes, result) = if wrapper == Wrapper::Output {
            let result = stream.write(&bytes);
            (bytes, result)
        } else {
            policy
                .run(move || {
                    let result = write_bytes(&mut stream, &bytes);
                    (bytes, result)
                })
                .await?
        };
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
