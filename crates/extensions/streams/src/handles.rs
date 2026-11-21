//! `fopen('hello.txt', 'r')` is a resource shared by aliases until `fclose`.

use std::io::{self, SeekFrom, Write};

use phera_extension_api::{
    ConstantValue, Context, FunctionEntry, LocalFuture, NativeError, NativeResult, Param,
    ParamType, Registry, Value, args, value::Resource,
};

use crate::context_options::{OPTIONAL_CONTEXT, validate};
use crate::errors::{check_path, nonnegative, reason};
use crate::io_policy::{CHUNK, IoPolicy};
use crate::request::{files, new_resource};
use crate::resource::{argument_resource, operate, stream_arg, with_stream};
use crate::stream::Wrapper;
use crate::wrapper::{Target, open};

pub const STREAM: Param = Param::new("stream", ParamType::Mixed);
pub const FILENAME: Param = Param::new("filename", ParamType::String);
pub const INCLUDE_PATH: Param =
    Param::new("use_include_path", ParamType::Bool).with_default(ConstantValue::Bool(false));
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
            FunctionEntry::asynchronous("fclose", const { &[STREAM] }, fclose),
            FunctionEntry::asynchronous("feof", const { &[STREAM] }, feof),
            FunctionEntry::sync("ftell", const { &[STREAM] }, ftell),
            FunctionEntry::asynchronous(
                "fseek",
                const {
                    &[
                        STREAM,
                        Param::new("offset", ParamType::Int),
                        Param::new("whence", ParamType::Int).with_default(ConstantValue::Int(0)),
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

        let mode = args::bytes(&arguments, 1);
        let include_path = args::bool(&arguments, 2).unwrap_or(false);
        let stream_context = args::value(&arguments, 3);
        validate(stream_context, "fopen", 4)?;
        if crate::filters::is_filter_uri(name)
            && let Some(result) = Box::pin(crate::filters::open_request_filter(
                context,
                "fopen",
                name,
                mode,
                include_path,
                stream_context,
            ))
            .await?
        {
            return Ok(result);
        }
        open_resource(context, "fopen", name, mode, include_path, stream_context).await
    })
}

pub(crate) async fn open_resource(
    context: &mut dyn Context,
    function: &str,
    name: &[u8],
    mode: &[u8],
    include_path: bool,
    stream_context: &Value,
) -> NativeResult {
    files(context).use_default_context(stream_context);
    if let Some(wrapper) = crate::user_wrappers::locate(context, name) {
        let supplied = if stream_context.is_null() {
            crate::context_options::default_for_wrapper(context)
        } else {
            stream_context.clone()
        };
        let flags = if include_path {
            crate::user_wrappers::USE_PATH
        } else {
            0
        };
        let opened =
            crate::user_file::open(context, wrapper, function, name, mode, flags, supplied).await?;
        return Ok(match opened {
            Some(stream) => crate::user_resource::make_file(context, stream),
            None => Value::Bool(false),
        });
    }
    if crate::socket::http::is_url(name) {
        return crate::socket::http::fopen(context, function, name, mode, stream_context).await;
    }
    let stream = if let Target::Gzip(filename) = Target::parse(name)
        && !crate::user_wrappers::protocol_disabled(context, b"compress.zlib")
        && let Some(level) = crate::context_options::option(stream_context, "zlib", "level")
    {
        let level = phera_extension_api::value::to_int(&level, context.diagnostics())?;
        let level = u32::try_from(level).ok().filter(|level| *level <= 9);
        crate::gzip::open_gzip(
            context,
            function,
            filename,
            name,
            mode,
            crate::gzip::GzipOptions {
                include_path,
                level,
                wrapper: Wrapper::Gzip,
            },
        )
        .await?
    } else {
        open(context, function, name, mode, include_path).await?
    };
    Ok(match stream {
        Some(stream) => new_resource(context.state(), "stream", stream),
        None => Value::Bool(false),
    })
}

fn fclose(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(close_arguments(context, arguments, "fclose"))
}

pub(crate) async fn close_arguments(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let candidate = argument_resource(args::value(&arguments, 0), function, "#1 ($stream)")?;
    if crate::directory::is_open(candidate) {
        context.warning(format!(
            "{function}(): cannot close the provided stream, as it must not be manually closed"
        ));
        return Ok(Value::Bool(false));
    }
    let resource = stream_arg(&arguments, function)?;
    if let Some(chains) = crate::filters::user::lookup(context, resource.id) {
        if chains.is_busy() {
            context.warning(format!(
                "{function}(): cannot close the provided stream, as it must not be manually closed"
            ));
            return Ok(Value::Bool(false));
        }
        crate::filters::user::close(context, resource).await?;
    }
    if let Some(file) = crate::user_file::resource_file(resource) {
        if file.busy() {
            context.warning(format!(
                "{function}(): cannot close the provided stream, as it must not be manually closed"
            ));
            return Ok(Value::Bool(false));
        }
        crate::user_resource::release(context, resource);
        file.close(context).await?;
        return Ok(Value::Bool(true));
    }
    finish_native(context, resource, function).await
}

pub(crate) async fn finish_native(
    context: &mut dyn Context,
    resource: &Resource,
    function: &str,
) -> NativeResult {
    let (gzip, filtered) = with_stream(resource, |stream| {
        if let crate::stream::Backing::Socket(socket) = &stream.backing {
            socket.retire();
        }
        (
            stream.wrapper.is_gzip(),
            stream
                .filters
                .as_ref()
                .is_some_and(|filters| filters.write.active()),
        )
    })?;
    if filtered {
        let result = operate(resource, IoPolicy::Blocking, |stream| {
            stream.filter_write(&[], crate::filters::Flush::Closing)
        })
        .await?;
        match result {
            Ok(output) if !output.is_empty() => context.write(&output),
            Ok(_) => (),
            Err(error) => context.warning(format!("{function}(): {error}")),
        }
        with_stream(resource, |stream| stream.filters = None)?;
    }
    let success = if gzip {
        operate(resource, IoPolicy::Blocking, |stream| stream.finish())
            .await?
            .is_ok()
    } else {
        true
    };
    let attachment = crate::child_pipe::take_attachment(resource)?;
    files(context).stream_contexts.remove(&resource.id);
    if let Some(attachment) = attachment {
        attachment.close(context).await?;
    }
    Ok(Value::Bool(success))
}

fn feof(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let resource = stream_arg(&arguments, "feof")?;
        if crate::filters::user::lookup(context, resource.id)
            .is_some_and(|chain| chain.has_buffered())
        {
            return Ok(Value::Bool(false));
        }
        if let Some(file) = crate::user_file::resource_file(resource) {
            return Ok(Value::Bool(file.at_end(context, "feof").await?));
        }
        with_stream(resource, |stream| {
            let eof = match &stream.backing {
                crate::stream::Backing::Socket(socket) => socket.eof(),
                _ => stream.eof,
            };
            Value::Bool(eof)
        })
    })
}

fn ftell(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let resource = stream_arg(arguments, "ftell")?;
    if let Some(file) = crate::user_file::resource_file(resource) {
        return Ok(Value::Int(file.position.get()));
    }
    with_stream(resource, |stream| Value::Int(stream.position as i64))
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
    Box::pin(seek_arguments(context, arguments, "fseek"))
}

pub(crate) async fn seek_arguments(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let offset = args::int(&arguments, 1).unwrap_or(0);
    let whence = args::int(&arguments, 2).unwrap_or(0);
    let candidate = argument_resource(args::value(&arguments, 0), function, "#1 ($stream)")?;
    if crate::directory::is_open(candidate) {
        if offset == 0 && whence == 0 {
            crate::directory::rewind(context, candidate).await?;
            return Ok(Value::Int(0));
        }
        return Ok(Value::Int(-1));
    }
    let resource = stream_arg(&arguments, function)?;
    if let Some(file) = crate::user_file::resource_file(resource) {
        let success = file.seek(context, function, offset, whence).await?;
        return Ok(Value::Int(if success { 0 } else { -1 }));
    }
    let policy = with_stream(resource, |stream| stream.seek_policy())?;
    if whence == 2 && with_stream(resource, |stream| stream.wrapper.is_gzip())? {
        context.warning(format!("{function}(): SEEK_END is not supported"));
        return Ok(Value::Int(-1));
    }
    let seek = seek_from(offset, whence);
    let success = match seek {
        Some(seek) => operate(resource, policy, move |stream| stream.seek(seek))
            .await?
            .is_ok(),
        None => false,
    };
    if !success && !with_stream(resource, |stream| stream.seekable)? {
        context.warning(format!("{function}(): Stream does not support seeking"));
    }
    Ok(Value::Int(if success { 0 } else { -1 }))
}

fn rewind(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(rewind_arguments(context, arguments, "rewind"))
}

pub(crate) async fn rewind_arguments(
    context: &mut dyn Context,
    arguments: Vec<Value>,
    function: &str,
) -> NativeResult {
    let resource = stream_arg(&arguments, function)?;
    if let Some(file) = crate::user_file::resource_file(resource) {
        return Ok(Value::Bool(file.seek(context, function, 0, 0).await?));
    }
    let (seekable, immediate) = with_stream(resource, |stream| {
        let seekable = stream.seekable;
        let result = matches!(stream.seek_policy(), IoPolicy::Inline)
            .then(|| stream.seek(SeekFrom::Start(0)).is_ok());
        (seekable, result)
    })?;
    let success = match immediate {
        Some(result) => result,
        None => operate(resource, IoPolicy::Blocking, |stream| {
            stream.seek(SeekFrom::Start(0))
        })
        .await?
        .is_ok(),
    };
    if !seekable {
        context.warning(format!("{function}(): Stream does not support seeking"));
    }
    Ok(Value::Bool(success))
}

fn ftruncate(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let stream = stream_arg(&arguments, "ftruncate")?;
        let size = nonnegative(
            "ftruncate",
            "#2 ($size)",
            args::int(&arguments, 1).unwrap_or(0),
        )?;
        if let Some(file) = crate::user_file::resource_file(stream) {
            return Ok(Value::Bool(
                file.truncate(context, "ftruncate", size).await?,
            ));
        }
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
        if matches!(self, Self::Buffers)
            && let Some(chains) = crate::filters::user::lookup(context, stream.id)
            && chains.writing()
            && !chains.flush(context, stream, function).await?
        {
            return Ok(Value::Bool(false));
        }
        if let Some(file) = crate::user_file::resource_file(stream) {
            if matches!(self, Self::Buffers) {
                return Ok(Value::Bool(file.flush(context).await?));
            }
            context.warning(format!("{function}(): Can't fsync this stream!"));
            return Ok(Value::Bool(false));
        }
        let result = operate(stream, IoPolicy::Blocking, move |stream| {
            let output = if matches!(self, Self::Buffers)
                && stream
                    .filters
                    .as_ref()
                    .is_some_and(|filters| filters.write.active())
            {
                stream.filter_write(&[], crate::filters::Flush::Incremental)?
            } else {
                Vec::new()
            };
            if let crate::stream::Backing::Gzip(gzip) = &mut stream.backing {
                return if matches!(self, Self::Buffers) {
                    gzip.flush().map(|()| output)
                } else {
                    Err(io::ErrorKind::Unsupported.into())
                };
            }
            let Some(file) = stream.descriptor() else {
                return match self {
                    Self::Buffers => Ok(output),
                    _ => Err(io::ErrorKind::Unsupported.into()),
                };
            };
            match self {
                Self::Buffers => (&*file).flush().map(|()| output),
                Self::Data => file.sync_data().map(|()| output),
                Self::All => file.sync_all().map(|()| output),
            }
        })
        .await?;
        match result {
            Ok(output) => {
                if !output.is_empty() {
                    context.write(&output);
                }
                Ok(Value::Bool(true))
            }
            Err(error) => {
                if !matches!(self, Self::Buffers) {
                    context.warning(format!("{function}(): Can't fsync this stream!"));
                } else if error.kind() == io::ErrorKind::Other {
                    context.warning(format!("{function}(): {error}"));
                }
                Ok(Value::Bool(false))
            }
        }
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
    if error.kind() == io::ErrorKind::TimedOut {
        return;
    }
    if error.kind() == io::ErrorKind::Other && error.get_ref().is_some() {
        context.warning(format!("{function}(): {error}"));
    } else if let Some(errno) = error.raw_os_error() {
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
    if error.kind() == io::ErrorKind::Other && error.get_ref().is_some() {
        context.warning(format!("{function}(): {error}"));
    } else if matches!(wrapper, Wrapper::Data | Wrapper::Input) {
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
