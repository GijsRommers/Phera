//! SplFileObject applies its stored CSV controls to direct reads and iteration.
//!
//! ```php
//! $file->setCsvControl(';', '"', '');
//! $row = $file->fgetcsv();
//! ```

use phera_ext_standard::csv::{Controls as CsvControls, Dialect};
use phera_extension_api::{Context, LocalFuture, NativeResult, Object, PhpString, Value, args};
use std::rc::Rc;

use super::{DROP_NEW_LINE, READ_CSV, SKIP_EMPTY, change, get, stream_call};
use crate::{This, list, receiver};

#[derive(Clone, Copy)]
pub(super) struct Controls {
    pub bytes: CsvControls,
    default_escape: bool,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            bytes: CsvControls::default(),
            default_escape: true,
        }
    }
}

impl Controls {
    fn arguments(
        self,
        context: &mut dyn Context,
        args: &[Value],
        offset: usize,
        method: &str,
    ) -> Result<CsvControls, phera_extension_api::NativeError> {
        let reason = self.default_escape.then_some(concat!(
            ", as its default value will change, either explicitly or via ",
            "SplFileObject::setCsvControl()"
        ));
        self.bytes
            .with_arguments(context, method, args, offset, reason)
    }
}

pub(super) fn set(context: &mut dyn Context, this: This<'_>, args: &[Value]) -> NativeResult {
    let object = receiver(this)?;
    get(object, |_| ())?;
    let bytes = CsvControls::default().with_arguments(
        context,
        "SplFileObject::setCsvControl",
        args,
        0,
        Some(" as its default value will change"),
    )?;
    change(object, |file| {
        file.csv.bytes = bytes;
        if args::string(args, 2).is_some() {
            file.csv.default_escape = false;
        }
    })?;
    Ok(Value::Null)
}

pub(super) fn read_controls(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |file| {
        list(file.csv.bytes.strings().into_iter().map(Value::String))
    })
}

pub(super) async fn read(object: &Object, dialect: Dialect) -> NativeResult {
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
    let mut add = i64::from(add);
    while let Some(record) = stream.read_csv(limit, dialect).await? {
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
        let controls = get(object, |file| file.csv)?.arguments(
            context,
            &arguments,
            0,
            "SplFileObject::fgetcsv",
        )?;
        read(object, controls.dialect(context)).await
    })
}

pub(super) fn put_csv(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let (stream, stored) = get(object, |file| (file.stream.clone(), file.csv))?;
        let controls = stored.arguments(context, &arguments, 1, "SplFileObject::fputcsv")?;
        let [separator, enclosure, escape] = controls.arguments();
        let eol = args::string(&arguments, 4)
            .map_or(Value::from("\n"), |string| Value::String(string.clone()));
        stream_call(
            context,
            &stream,
            "fputcsv",
            vec![
                args::value(&arguments, 0).clone(),
                separator,
                enclosure,
                escape,
                eol,
            ],
        )
        .await
    })
}
