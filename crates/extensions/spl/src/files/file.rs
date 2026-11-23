//! SplFileObject owns a stream and the independent PHP line-iterator state.
//!
//! ```php
//! $file = new SplFileObject('report.txt');
//! $file->seek(2);
//! echo $file->current();
//! ```

mod csv;
mod lines;
mod operations;
mod registration;

pub(super) use registration::{OPEN_PARAMS, register};

use phera_ext_streams::native::{self, FileStream};
use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpArray, PhpString, Value, args,
};
use std::rc::Rc;

use super::{Data, FILE, Location, information, read, update};
use crate::receiver;

const TEMP: &str = "SplTempFileObject";
const DROP_NEW_LINE: i64 = 1;
const READ_AHEAD: i64 = 2;
const SKIP_EMPTY: i64 = 4;
const READ_CSV: i64 = 8;
const FLAGS_MASK: i64 = DROP_NEW_LINE | READ_AHEAD | SKIP_EMPTY | READ_CSV;

#[derive(Clone)]
pub(super) struct File {
    pub(super) location: Location,
    stream: FileStream,
    mode: PhpString,
    csv: csv::Controls,
    max_line: Option<u64>,
    flags: i64,
    number: i64,
    current: Option<Value>,
    text: Option<PhpString>,
}

impl File {
    pub(super) fn debug(&self, props: &mut PhpArray) {
        let [separator, enclosure, _] = self.csv.bytes.strings();
        for (key, value) in [
            ("openMode", self.mode.clone()),
            ("delimiter", separator),
            ("enclosure", enclosure),
        ] {
            information::private(props, FILE, key, Value::String(value));
        }
    }
}

fn get<T>(object: &Object, f: impl FnOnce(&File) -> T) -> Result<T, NativeError> {
    read(object, |state| match &state.data {
        Data::File(file) => Some(f(file)),
        _ => None,
    })?
    .ok_or_else(|| NativeError::error("Object not initialized"))
}

fn change<T>(object: &Object, f: impl FnOnce(&mut File) -> T) -> Result<T, NativeError> {
    update(object, |state| match &mut state.data {
        Data::File(file) => Some(f(file)),
        _ => None,
    })?
    .ok_or_else(|| NativeError::error("Object not initialized"))
}

async fn initialize(
    context: &mut dyn Context,
    object: &Object,
    mut location: Location,
    mode: PhpString,
    include_path: bool,
    stream_context: &Value,
    method: &str,
) -> NativeResult {
    if read(object, |state| matches!(state.data, Data::File(_)))? {
        return Err(NativeError::error("Cannot call constructor twice"));
    }
    let name = location.pathname.as_bytes();
    native::check_path(method, "#1 ($filename)", name)?;
    let stream = FileStream::open(
        context,
        method,
        name,
        mode.as_bytes(),
        include_path,
        stream_context,
    )
    .await?;
    if object.class.is_instance_of(TEMP) {
        location.path = PhpString::from("");
    }
    update(object, |state| {
        state.data = Data::File(Box::new(File {
            location,
            stream,
            mode,
            csv: csv::Controls::default(),
            max_line: None,
            flags: 0,
            current: None,
            text: None,
            number: 0,
        }))
    })?;
    Ok(Value::Null)
}

fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::bytes(&arguments, 0);
        native::check_path("SplFileObject::__construct", "#1 ($filename)", name)?;
        let location = Location::info(name);
        let mode =
            args::string(&arguments, 1).map_or_else(|| PhpString::from("r"), |s| s.to_php_string());
        initialize(
            context,
            receiver(this.as_ref())?,
            location,
            mode,
            args::bool(&arguments, 2).unwrap_or_default(),
            args::value(&arguments, 3),
            "SplFileObject::__construct",
        )
        .await
    })
}

fn construct_temp(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = match args::int(&arguments, 0) {
            None => PhpString::from("php://temp"),
            Some(limit) if limit < 0 => PhpString::from("php://memory"),
            Some(limit) => PhpString::from(format!("php://temp/maxmemory:{limit}")),
        };
        initialize(
            context,
            receiver(this.as_ref())?,
            Location {
                path: PhpString::from(""),
                pathname: name,
            },
            PhpString::from("wb"),
            false,
            &Value::Null,
            "SplTempFileObject::__construct",
        )
        .await
    })
}

pub(super) fn open_file(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let (location, class, valid) = read(object, |state| {
            let valid = !matches!(&state.data, Data::Directory(dir) if dir.entry().is_none());
            (state.location(), state.classes.file.clone(), valid)
        })?;
        if !valid {
            return Err(NativeError::throw(
                "RuntimeException",
                "Could not open file",
            ));
        }
        let location = location?;
        let mode =
            args::string(&arguments, 0).map_or_else(|| PhpString::from("r"), |s| s.to_php_string());
        let result = context.create_object(&class, PhpArray::new())?;
        let Value::Object(file) = &result else {
            return Err(NativeError::error("File factory must return an object"));
        };
        if context.is_user_method(file, "__construct") || file.class.is_instance_of(TEMP) {
            context
                .call_method(
                    file.clone(),
                    "__construct",
                    vec![Value::String(location.pathname), Value::String(mode)],
                )
                .await?;
        } else {
            initialize(
                context,
                file,
                location,
                mode,
                args::bool(&arguments, 1).unwrap_or_default(),
                args::value(&arguments, 2),
                "SplFileInfo::openFile",
            )
            .await?;
        }
        Ok(result)
    })
}

async fn stream_call(
    context: &mut dyn Context,
    stream: &FileStream,
    name: &str,
    args: Vec<Value>,
) -> NativeResult {
    let mut values = Vec::with_capacity(args.len() + 1);
    values.push(stream.resource());
    values.extend(args);
    context.call(&Value::from(name), values).await
}
