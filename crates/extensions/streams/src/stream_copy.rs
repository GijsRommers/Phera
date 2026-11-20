//! `stream_copy_to_stream($input, $output, 16)` copies up to sixteen bytes.

use std::io::SeekFrom;
use std::rc::Rc;

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args, value::Resource,
};

use crate::errors::warn_false;
use crate::handles::{LENGTH, STREAM, read_amount, read_notice};
use crate::io_policy::{CHUNK, IoPolicy};
use crate::resource::{operate, stream_arg, stream_argument, with_stream};
use crate::transfer::{read, write};

pub fn register(registry: &mut Registry) {
    const OFFSET: Param = Param::new("offset", ParamType::Int).optional();
    const GET: &[Param] = &[STREAM, LENGTH, OFFSET];
    const COPY: &[Param] = &[
        Param::new("from", ParamType::Mixed),
        Param::new("to", ParamType::Mixed),
        LENGTH,
        OFFSET,
    ];
    registry.function(FunctionEntry::asynchronous(
        "stream_get_contents",
        GET,
        get_contents,
    ));
    registry.function(FunctionEntry::asynchronous(
        "stream_copy_to_stream",
        COPY,
        copy_to_stream,
    ));
}

fn get_contents(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = "stream_get_contents";
        let resource = stream_arg(&arguments, name)?;
        let length = args::int(&arguments, 1).unwrap_or(-1);
        if length < -1 {
            return Err(NativeError::value_error(
                "stream_get_contents(): Argument #2 ($length) must be greater than or equal to -1",
            ));
        }
        let length = u64::try_from(length).ok();
        let offset = args::int(&arguments, 2).unwrap_or(-1);
        if offset >= 0 && !seek(context, resource, name, offset as u64, Seek::Current).await? {
            return Ok(Value::Bool(false));
        }
        let result = if let Some(pipe) = crate::child_pipe::of(resource)? {
            crate::child_pipe::bounded(context, resource, pipe.contents(resource, length)).await??
        } else {
            if let Some(file) = crate::user_file::resource_file(resource) {
                Ok(file.collect(context, name, length).await?)
            } else {
                read(resource, length).await?
            }
        };
        match result {
            Ok(data) => Ok(Value::from(data)),
            Err(error) => {
                read_notice(context, name, &error, read_amount(length));
                Ok(Value::from(""))
            }
        }
    })
}

fn copy_to_stream(
    context: &mut dyn Context,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = "stream_copy_to_stream";
        let from = stream_argument(args::value(&arguments, 0), name, "#1 ($from)")?;
        let to = stream_argument(args::value(&arguments, 1), name, "#2 ($to)")?;
        let offset = args::int(&arguments, 3).unwrap_or(0);
        if offset > 0 && !seek(context, from, name, offset as u64, Seek::Absolute).await? {
            return Ok(Value::Bool(false));
        }
        let length = args::int(&arguments, 2).unwrap_or(-1) as u64;
        if Rc::ptr_eq(from, to) {
            return if length == 0 {
                Ok(Value::Int(0))
            } else {
                Ok(Value::Bool(false))
            };
        }
        copy(context, from, to, name, length).await
    })
}

pub async fn copy(
    context: &mut dyn Context,
    from: &Resource,
    to: &Resource,
    name: &str,
    length: u64,
) -> NativeResult {
    let mut remaining = length;
    while remaining > 0 {
        let requested = remaining.min(CHUNK as u64);
        // A long transfer also yields when each individual chunk could run inline.
        let result = if let Some(file) = crate::user_file::resource_file(from) {
            file.read(context, name, requested as usize, false).await?
        } else {
            let policy = if length > crate::io_policy::INLINE_LIMIT {
                IoPolicy::Blocking
            } else {
                with_stream(from, |stream| stream.read_policy(Some(requested)))?
            };
            if crate::child_pipe::of(from)?.is_some() {
                crate::child_pipe::bounded(context, from, read(from, Some(requested))).await??
            } else {
                operate(from, policy, move |stream| stream.read(Some(requested))).await?
            }
        };
        let bytes = match result {
            Ok(bytes) if bytes.is_empty() => break,
            Ok(bytes) => bytes,
            Err(error) => {
                read_notice(context, name, &error, requested);
                return Ok(Value::Bool(false));
            }
        };
        match write(context, to, &bytes, name).await? {
            Value::Int(written) if written as usize == bytes.len() => remaining -= written as u64,
            _ => return Ok(Value::Bool(false)),
        }
    }
    Ok(Value::Int((length - remaining) as i64))
}

enum Seek {
    Absolute,
    Current,
}

async fn seek(
    context: &mut dyn Context,
    resource: &Resource,
    function: &str,
    offset: u64,
    kind: Seek,
) -> Result<bool, NativeError> {
    if let Some(file) = crate::user_file::resource_file(resource) {
        if matches!(kind, Seek::Current) && file.position.get() == offset as i64 {
            return Ok(true);
        }
        if file.seek(context, function, offset as i64, 0).await? {
            return Ok(true);
        }
        warn_false(
            context,
            format!("{function}(): Failed to seek to position {offset} in the stream"),
        )?;
        return Ok(false);
    }
    let (position, seekable) = with_stream(resource, |stream| (stream.position, stream.seekable))?;
    if matches!(kind, Seek::Current) && offset == position {
        return Ok(true);
    }
    if seekable {
        if operate(resource, IoPolicy::Inline, move |stream| {
            stream.seek(SeekFrom::Start(offset))
        })
        .await?
        .is_ok()
        {
            return Ok(true);
        }
    } else if matches!(kind, Seek::Current) && offset > position {
        let remaining = offset - position;
        let skipped = operate(resource, IoPolicy::Blocking, move |stream| {
            let mut remaining = remaining;
            while remaining > 0 {
                let chunk = stream.read(Some(remaining.min(CHUNK as u64)))?;
                if chunk.is_empty() {
                    return Ok::<_, std::io::Error>(false);
                }
                remaining -= chunk.len() as u64;
            }
            Ok(true)
        })
        .await?;
        if skipped.unwrap_or(false) {
            return Ok(true);
        }
    } else {
        context.warning(format!("{function}(): Stream does not support seeking"));
    }
    warn_false(
        context,
        format!("{function}(): Failed to seek to position {offset} in the stream"),
    )?;
    Ok(false)
}
