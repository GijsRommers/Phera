//! File iteration keeps the current line independently of the stream's byte position.
//!
//! ```php
//! $file->setFlags(SplFileObject::READ_AHEAD | SplFileObject::DROP_NEW_LINE);
//! foreach ($file as $number => $text) {}
//! ```

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpString, Value, args, value::to_bool,
};
use std::rc::Rc;

use super::{DROP_NEW_LINE, File, READ_AHEAD, READ_CSV, SKIP_EMPTY, change, csv, get, stream_call};
use crate::{This, receiver};

mod settings;
pub(super) use settings::{flags, max_line, set_flags, set_max_line};

#[derive(Clone, Copy)]
pub(super) enum LineNumber {
    AlwaysAdvance,
    AdvanceAfterBuffered,
}

pub(super) fn clear(file: &mut File) {
    file.text = None;
    file.current = None;
}

fn unreadable(path: &PhpString) -> NativeError {
    NativeError::throw(
        "RuntimeException",
        format!(
            "Cannot read from file {}",
            String::from_utf8_lossy(path.as_bytes())
        ),
    )
}

pub(super) async fn raw_line(
    object: &Object,
    numbering: LineNumber,
    require_line: bool,
) -> NativeResult {
    let (stream, path, limit, flags, buffered) = get(object, |file| {
        (
            file.stream.clone(),
            file.location.pathname.clone(),
            file.max_line,
            file.flags,
            file.text.is_some() || file.current.is_some(),
        )
    })?;
    change(object, clear)?;
    if stream.eof()? {
        return if require_line {
            Err(unreadable(&path))
        } else {
            Ok(Value::Bool(false))
        };
    }
    let line = stream.read_line(limit).await?.unwrap_or_default();
    let mut line = line.as_slice();
    if flags & DROP_NEW_LINE != 0
        && let Some(without_lf) = line.strip_suffix(b"\n")
    {
        line = without_lf.strip_suffix(b"\r").unwrap_or(without_lf);
    }
    let line = PhpString::from(line);
    let advance = matches!(numbering, LineNumber::AlwaysAdvance) || buffered;
    change(object, |file| {
        file.number = file.number.saturating_add(i64::from(advance));
        file.text = Some(line.clone());
        file.current = Some(Value::String(line.clone()));
    })?;
    Ok(Value::String(line))
}

async fn iteration_line(
    context: &mut dyn Context,
    object: &Rc<Object>,
    require_line: bool,
) -> NativeResult {
    let (flags, controls, stream, path) = get(object, |file| {
        (
            file.flags,
            file.csv,
            file.stream.clone(),
            file.location.pathname.clone(),
        )
    })?;
    if flags & READ_CSV != 0 {
        let row = csv::read(object, controls.bytes.dialect(context)).await?;
        if require_line && matches!(row, Value::Bool(false)) {
            return Err(unreadable(&path));
        }
        return Ok(row);
    }
    let overridden = context.is_user_method(object, "getCurrentLine");
    loop {
        let line = if overridden {
            if stream.eof()? {
                return if require_line {
                    Err(unreadable(&path))
                } else {
                    Ok(Value::Bool(false))
                };
            }
            change(object, clear)?;
            let result = context
                .call_method(object.clone(), "getCurrentLine", Vec::new())
                .await?;
            let Value::String(text) = &*result.deref() else {
                return Err(NativeError::type_error(format!(
                    "{}::getCurrentLine(): Return value must be of type string, {} returned",
                    object.class.name(),
                    result.type_name()
                )));
            };
            change(object, |file| {
                file.text = Some(text.clone());
                file.current = Some(Value::String(text.clone()));
            })?;
            Value::String(text.clone())
        } else {
            raw_line(object, LineNumber::AdvanceAfterBuffered, require_line).await?
        };
        if flags & SKIP_EMPTY == 0
            || !matches!(&line, Value::String(text) if text.as_bytes().is_empty())
        {
            return Ok(line);
        }
        change(object, clear)?;
    }
}

async fn rewind_file(context: &mut dyn Context, object: &Rc<Object>) -> Result<(), NativeError> {
    let (stream, path, read_ahead) = get(object, |file| {
        (
            file.stream.clone(),
            file.location.pathname.clone(),
            file.flags & READ_AHEAD != 0,
        )
    })?;
    if !to_bool(&stream_call(context, &stream, "rewind", Vec::new()).await?) {
        return Err(NativeError::throw(
            "RuntimeException",
            format!(
                "Cannot rewind file {}",
                String::from_utf8_lossy(path.as_bytes())
            ),
        ));
    }
    change(object, |file| {
        clear(file);
        file.number = 0;
    })?;
    if read_ahead {
        iteration_line(context, object, false).await?;
    }
    Ok(())
}

pub(super) fn rewind(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        rewind_file(context, receiver(this.as_ref())?).await?;
        Ok(Value::Null)
    })
}

pub(super) fn next(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let read_ahead = change(object, |file| {
            clear(file);
            file.flags & READ_AHEAD != 0
        })?;
        if read_ahead {
            iteration_line(context, object, false).await?;
        }
        change(object, |file| file.number = file.number.saturating_add(1))?;
        Ok(Value::Null)
    })
}

pub(super) fn current(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        if get(object, |file| file.current.is_none() && file.text.is_none())? {
            iteration_line(context, object, false).await?;
        }
        get(object, |file| {
            if file.flags & READ_CSV != 0 {
                file.current.clone().unwrap_or(Value::Bool(false))
            } else {
                file.text.clone().map_or(Value::Bool(false), Value::String)
            }
        })
    })
}

pub(super) fn string(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        if let Some(text) = get(object, |file| file.text.clone())? {
            return Ok(Value::String(text));
        }
        iteration_line(context, object, true).await?;
        get(object, |file| {
            file.text.clone().map_or(Value::from(""), Value::String)
        })
    })
}

pub(super) fn fgets(
    _: &mut dyn Context,
    this: Option<Rc<Object>>,
    _: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(
        async move { raw_line(receiver(this.as_ref())?, LineNumber::AlwaysAdvance, true).await },
    )
}

pub(super) fn seek(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let target = args::int(&arguments, 0).unwrap_or_default();
        let read_ahead = get(object, |file| file.flags & READ_AHEAD != 0)?;
        if target < 0 {
            return Err(NativeError::value_error(concat!(
                "SplFileObject::seek(): Argument #1 ($line) ",
                "must be greater than or equal to 0"
            )));
        }
        rewind_file(context, object).await?;
        for _ in 0..target {
            if matches!(
                iteration_line(context, object, false).await?,
                Value::Bool(false)
            ) {
                return Ok(Value::Null);
            }
        }
        if target > 0 && !read_ahead {
            change(object, |file| {
                file.number = file.number.saturating_add(1);
                clear(file);
            })?;
        }
        Ok(Value::Null)
    })
}

pub(super) fn key(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |file| Value::Int(file.number))
}

pub(super) fn eof(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |file| file.stream.clone())?
        .eof()
        .map(Value::Bool)
}

pub(super) fn valid(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    let status = super::read(object, |state| {
        if let super::Data::File(file) = &state.data {
            if file.flags & READ_AHEAD != 0 {
                return Ok(file.current.is_some() || file.text.is_some());
            }
            file.stream.eof().map(|value| !value)
        } else {
            Ok(false)
        }
    })??;
    Ok(Value::Bool(status))
}
