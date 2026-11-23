//! Byte-oriented methods share the file's stream without using a second handle.
//!
//! ```php
//! $file->fseek(0);
//! echo $file->fread(12);
//! ```

use phera_ext_streams::native;
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Object, Value, args};
use std::rc::Rc;

use super::{change, get, lines, stream_call};
use crate::{This, receiver};

fn operation<'a>(
    context: &'a mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
    name: &'static str,
) -> LocalFuture<'a, NativeResult> {
    Box::pin(async move {
        let stream = get(receiver(this.as_ref())?, |file| file.stream.clone())?;
        stream_call(context, &stream, name, arguments).await
    })
}

pub(super) fn tell(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    operation(context, this, arguments, "ftell")
}

pub(super) fn flush(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    operation(context, this, arguments, "fflush")
}

pub(super) fn stat(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    operation(context, this, arguments, "fstat")
}

pub(super) fn lock(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    operation(context, this, arguments, "flock")
}

pub(super) fn passthru(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    operation(context, this, arguments, "fpassthru")
}

pub(super) fn seek(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let stream = change(object, |file| {
            lines::clear(file);
            file.stream.clone()
        })?;
        stream_call(context, &stream, "fseek", arguments).await
    })
}

pub(super) fn character(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let stream = change(object, |file| {
            lines::clear(file);
            file.stream.clone()
        })?;
        let Ok(bytes) = stream
            .read_bytes(context, "SplFileObject::fgetc", 1)
            .await?
        else {
            return Ok(Value::Bool(false));
        };
        if bytes.is_empty() {
            return Ok(Value::Bool(false));
        }
        if bytes == b"\n" {
            change(object, |file| file.number = file.number.saturating_add(1))?;
        }
        Ok(Value::from(bytes))
    })
}

pub(super) fn read(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let stream = get(receiver(this.as_ref())?, |file| file.stream.clone())?;
        let length = args::int(&arguments, 0).unwrap_or_default();
        if length <= 0 {
            return Err(NativeError::value_error(
                "SplFileObject::fread(): Argument #1 ($length) must be greater than 0",
            ));
        }
        Ok(stream
            .read_bytes(context, "SplFileObject::fread", length as u64)
            .await?
            .map_or(Value::Bool(false), Value::from))
    })
}

pub(super) fn write(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let stream = get(receiver(this.as_ref())?, |file| file.stream.clone())?;
        let bytes = args::bytes(&arguments, 0);
        let length = args::int(&arguments, 1).map_or(bytes.len(), |limit| {
            usize::try_from(limit.max(0))
                .unwrap_or(usize::MAX)
                .min(bytes.len())
        });
        if length == 0 {
            return Ok(Value::Int(0));
        }
        stream.write_bytes(context, &bytes[..length]).await
    })
}

pub(super) fn truncate(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let (stream, path) = get(receiver(this.as_ref())?, |file| {
            (file.stream.clone(), file.location.pathname.clone())
        })?;
        let limit = args::int(&arguments, 0).unwrap_or_default();
        let size = u64::try_from(limit).map_err(|_| {
            NativeError::value_error(concat!(
                "SplFileObject::ftruncate(): Argument #1 ($size) ",
                "must be greater than or equal to 0"
            ))
        })?;
        match stream.truncate(context, size).await? {
            Ok(()) => Ok(Value::Bool(true)),
            Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {
                Err(NativeError::throw(
                    "LogicException",
                    format!(
                        "Can't truncate file {}",
                        String::from_utf8_lossy(path.as_bytes())
                    ),
                ))
            }
            Err(_) => Ok(Value::Bool(false)),
        }
    })
}

pub(super) fn scan(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let line = lines::raw_line(
            context,
            receiver(this.as_ref())?,
            lines::LineNumber::AdvanceAfterBuffered,
            true,
        )
        .await?;
        match line {
            Value::String(line) => native::scan_line(line.as_bytes(), &arguments),
            _ => Ok(Value::Null),
        }
    })
}

pub(super) fn has_children(_: &mut dyn Context, _this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Bool(false))
}

pub(super) fn children(_: &mut dyn Context, _this: This<'_>, _: &[Value]) -> NativeResult {
    Ok(Value::Null)
}
