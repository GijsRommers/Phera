//! `fopen('hello.txt', 'r')` is a resource shared by aliases until `fclose`.

use std::io::{self, SeekFrom, Write};

use phera_extension_api::{
    Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param, ParamType, Registry,
    Value, args,
};

use crate::context_options::{OPTIONAL_CONTEXT, validate};
use crate::errors::{check_path, nonnegative, reason};
use crate::io_policy::{CHUNK, IoPolicy};
use crate::request::new_resource;
use crate::resource::{operate, stream_arg, with_stream};
use crate::stream::Wrapper;
use crate::wrapper::open;

pub const STREAM: Param = Param::new("stream", ParamType::Mixed);
pub const FILENAME: Param = Param::new("filename", ParamType::String);
pub const INCLUDE_PATH: Param = Param::new("use_include_path", ParamType::Bool).optional();
pub const LENGTH: Param = Param::new("length", ParamType::Int).optional().nullable();

pub fn register(registry: &mut Registry) {
    registry.function(
        const {
            FunctionEntry::asynchronous(
                "fopen",
                const {
                    &[
                        FILENAME,
                        Param::new("mode", ParamType::String),
                        INCLUDE_PATH,
                        OPTIONAL_CONTEXT,
                    ]
                },
                fopen,
            )
        },
    );
    for entry in const {
        [
            FunctionEntry::sync("fclose", const { &[STREAM] }, fclose),
            FunctionEntry::sync("feof", const { &[STREAM] }, feof),
            FunctionEntry::sync("ftell", const { &[STREAM] }, ftell),
            FunctionEntry::asynchronous(
                "fseek",
                const {
                    &[
                        STREAM,
                        Param::new("offset", ParamType::Int),
                        Param::new("whence", ParamType::Int).optional(),
                    ]
                },
                fseek,
            ),
            FunctionEntry::asynchronous("rewind", const { &[STREAM] }, rewind),
            FunctionEntry::asynchronous(
                "ftruncate",
                const { &[STREAM, Param::new("size", ParamType::Int)] },
                ftruncate,
            ),
            FunctionEntry::asynchronous("fflush", const { &[STREAM] }, fflush),
            FunctionEntry::asynchronous("fsync", const { &[STREAM] }, fsync),
            FunctionEntry::asynchronous("fdatasync", const { &[STREAM] }, fdatasync),
        ]
    } {
        registry.function(entry);
    }
}

fn fopen(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let name = args::bytes(&arguments, 0);
        check_path("fopen", "#1 ($filename)", name)?;
        validate(args::value(&arguments, 3), "fopen", 4)?;
        let stream = open(
            context,
            "fopen",
            name,
            args::bytes(&arguments, 1),
            args::bool(&arguments, 2).unwrap_or(false),
        )
        .await?;
        Ok(match stream {
            Some(stream) => new_resource(context.state(), "stream", stream),
            None => Value::Bool(false),
        })
    })
}

fn fclose(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    stream_arg(arguments, "fclose")?.data.borrow_mut().take();
    Ok(Value::Bool(true))
}

fn feof(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    with_stream(stream_arg(arguments, "feof")?, |stream| {
        Value::Bool(stream.eof)
    })
}

fn ftell(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    with_stream(stream_arg(arguments, "ftell")?, |stream| {
        Value::Int(stream.position as i64)
    })
}

pub fn seek_from(offset: i64, whence: i64) -> Option<SeekFrom> {
    match whence {
        0 => u64::try_from(offset).ok().map(SeekFrom::Start),
        1 => Some(SeekFrom::Current(offset)),
        2 => Some(SeekFrom::End(offset)),
        _ => None,
    }
}

fn fseek(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = stream_arg(&arguments, "fseek")?;
        let offset = args::int(&arguments, 1).unwrap_or(0);
        let whence = args::int(&arguments, 2).unwrap_or(0);
        let seek = seek_from(offset, whence);
        let success = match seek {
            Some(seek) => operate(resource, IoPolicy::Inline, move |stream| stream.seek(seek))
                .await?
                .is_ok(),
            None => false,
        };
        if !success && !with_stream(resource, |stream| stream.seekable)? {
            context.warning("fseek(): Stream does not support seeking".into());
        }
        Ok(Value::Int(if success { 0 } else { -1 }))
    })
}

fn rewind(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = stream_arg(&arguments, "rewind")?;
        let result = operate(resource, IoPolicy::Inline, |stream| {
            stream.seek(SeekFrom::Start(0))
        })
        .await?;
        if !with_stream(resource, |stream| stream.seekable)? {
            context.warning("rewind(): Stream does not support seeking".into());
        }
        Ok(Value::Bool(result.is_ok()))
    })
}

fn ftruncate(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let stream = stream_arg(&arguments, "ftruncate")?;
        let size = nonnegative(
            "ftruncate",
            "#2 ($size)",
            args::int(&arguments, 1).unwrap_or(0),
        )?;
        let result = operate(stream, IoPolicy::Blocking, move |stream| {
            stream.truncate(size)
        })
        .await?;
        if result
            .as_ref()
            .is_err_and(|error| error.kind() == io::ErrorKind::Unsupported)
        {
            context.warning("ftruncate(): Can't truncate this stream!".into());
        }
        Ok(Value::Bool(result.is_ok()))
    })
}

#[derive(Clone, Copy)]
enum Flush {
    Buffers,
    Data,
    All,
}

impl Flush {
    async fn run(
        self,
        context: &mut dyn Context,
        arguments: &[Value],
        function: &str,
    ) -> NativeResult {
        let stream = stream_arg(arguments, function)?;
        let result = operate(stream, IoPolicy::Blocking, move |stream| {
            let Some(file) = stream.descriptor() else {
                return match self {
                    Self::Buffers => Ok(()),
                    _ => Err(io::ErrorKind::Unsupported.into()),
                };
            };
            match self {
                Self::Buffers => (&*file).flush(),
                Self::Data => file.sync_data(),
                Self::All => file.sync_all(),
            }
        })
        .await?;
        if result.is_err() && !matches!(self, Self::Buffers) {
            context.warning(format!("{function}(): Can't fsync this stream!"));
        }
        Ok(Value::Bool(result.is_ok()))
    }
}

fn fflush(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { Flush::Buffers.run(context, &arguments, "fflush").await })
}

fn fsync(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { Flush::All.run(context, &arguments, "fsync").await })
}

fn fdatasync(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move { Flush::Data.run(context, &arguments, "fdatasync").await })
}

pub fn read_notice(context: &mut dyn Context, function: &str, error: &io::Error, amount: u64) {
    if let Some(errno) = error.raw_os_error() {
        context.notice(format!(
            "{function}(): Read of {amount} bytes failed with errno={errno} {}",
            reason(error)
        ));
    }
}

pub fn read_amount(length: Option<u64>) -> u64 {
    length.unwrap_or(CHUNK as u64).max(CHUNK as u64)
}

pub fn write_notice(
    context: &mut dyn Context,
    function: &str,
    wrapper: Wrapper,
    error: &io::Error,
    length: usize,
) {
    if matches!(wrapper, Wrapper::Data | Wrapper::Input) {
        context.notice(format!("{function}(): Stream is not writable"));
    } else if let Some(errno) = error.raw_os_error() {
        context.notice(format!(
            "{function}(): Write of {length} bytes failed with errno={errno} {}",
            reason(error)
        ));
    }
}

pub fn positive(function: &str, value: i64) -> Result<u64, NativeError> {
    if value <= 0 {
        return Err(NativeError::value_error(format!(
            "{function}(): Argument #2 ($length) must be greater than 0"
        )));
    }
    Ok(value as u64)
}
