//! SplFileObject applies its stored CSV controls to direct reads and iteration.
//!
//! ```php
//! $file->setCsvControl(';', '"', '');
//! $row = $file->fgetcsv();
//! ```

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpString, Value, args,
};
use std::rc::Rc;

use super::{DROP_NEW_LINE, READ_CSV, SKIP_EMPTY, change, get, stream_call};
use crate::{This, list, receiver};

#[derive(Clone)]
pub(super) struct Controls {
    pub separator: PhpString,
    pub enclosure: PhpString,
    escape: PhpString,
    default_escape: bool,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            separator: ",".into(),
            enclosure: "\"".into(),
            escape: "\\".into(),
            default_escape: true,
        }
    }
}

fn controls(
    context: &mut dyn Context,
    args: &[Value],
    offset: usize,
    mut defaults: Controls,
    method: &str,
) -> Result<Controls, NativeError> {
    for (pos, name, target) in [
        (offset, "separator", &mut defaults.separator),
        (offset + 1, "enclosure", &mut defaults.enclosure),
        (offset + 2, "escape", &mut defaults.escape),
    ] {
        let Some(value) = args::string(args, pos) else {
            if name == "escape" && defaults.default_escape {
                let reason = if method == "setCsvControl" {
                    " as its default value will change"
                } else {
                    concat!(
                        ", as its default value will change, either explicitly or via ",
                        "SplFileObject::setCsvControl()"
                    )
                };
                context.deprecated(format!(
                    "SplFileObject::{method}(): the $escape parameter must be provided{reason}"
                ));
            }
            continue;
        };
        let count = value.as_bytes().len();
        if count != 1 && !(name == "escape" && count == 0) {
            let allowed = if name == "escape" {
                "empty or a single character"
            } else {
                "a single character"
            };
            return Err(NativeError::value_error(format!(
                "SplFileObject::{method}(): Argument #{} (${name}) must be {allowed}",
                pos + 1
            )));
        }
        if name == "escape" {
            defaults.default_escape = false;
        }
        *target = value.clone();
    }
    Ok(defaults)
}

pub(super) fn set(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    get(object, |_| ())?;
    let next = controls(context, args, 0, Controls::default(), "setCsvControl")?;
    change(object, |file| file.csv = next)?;
    Ok(Value::Null)
}

pub(super) fn read_controls(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |file| {
        list(
            [&file.csv.separator, &file.csv.enclosure, &file.csv.escape]
                .into_iter()
                .cloned()
                .map(Value::String),
        )
    })
}

fn first(value: &PhpString, default: u8) -> u8 {
    value.as_bytes().first().copied().unwrap_or(default)
}

pub(super) async fn read(object: &Object, controls: Controls) -> NativeResult {
    let (stream, limit, flags, add) = get(object, |file| {
        (
            file.stream.clone(),
            file.max_line,
            file.flags,
            file.current.is_some() || file.text.is_some(),
        )
    })?;
    change(object, |file| {
        file.current = None;
        file.text = None;
    })?;
    let separator = first(&controls.separator, b',');
    let enclosure = first(&controls.enclosure, b'"');
    let escape = controls.escape.as_bytes().first().copied();
    let mut add = i64::from(add);
    while let Some(record) = stream.read_csv(limit, separator, enclosure, escape).await? {
        let dropping = flags & (DROP_NEW_LINE | READ_CSV) == (DROP_NEW_LINE | READ_CSV);
        let line = record.line.as_slice();
        let blank = line.is_empty() || (dropping && matches!(line, b"\n" | b"\r\n"));
        if blank && flags & SKIP_EMPTY != 0 {
            add = add.saturating_add(1);
            continue;
        }
        let record_value = Value::from(record.fields);
        change(object, |file| {
            file.number = file.number.saturating_add(add);
            file.text = Some(PhpString::from(record.line));
            file.current = Some(record_value.clone());
        })?;
        return Ok(record_value);
    }
    Ok(Value::Bool(false))
}

pub(super) fn get_csv(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let dialect = controls(
            context,
            &arguments,
            0,
            get(object, |file| file.csv.clone())?,
            "fgetcsv",
        )?;
        read(object, dialect).await
    })
}

pub(super) fn put_csv(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let (stream, stored) = get(object, |file| (file.stream.clone(), file.csv.clone()))?;
        let dialect = controls(context, &arguments, 1, stored, "fputcsv")?;
        let eol = args::string(&arguments, 4)
            .map_or(Value::from("\n"), |string| Value::String(string.clone()));
        stream_call(
            context,
            &stream,
            "fputcsv",
            vec![
                args::value(&arguments, 0).clone(),
                Value::String(dialect.separator),
                Value::String(dialect.enclosure),
                Value::String(dialect.escape),
                eol,
            ],
        )
        .await
    })
}
